mod call_extractor;
mod has_delegatecall;
pub mod initialize_extractor;

use std::{
    io::{BufRead, BufReader},
    sync::{atomic::AtomicI32, Arc},
    thread,
};

use clap::{command, Parser};
use crossbeam::{channel, sync::WaitGroup};
use has_delegatecall::HasDelegateCallOrNot;
use libsofl_core::{
    blockchain::{
        provider::{BcProvider, BcStateProvider},
        transaction::Tx,
    },
    conversion::ConvertTo,
    engine::{
        state::BcState,
        transition::TransitionSpecBuilder,
        types::{Address, Bytes, TxHash},
    },
    error::SoflError,
};
use libsofl_reth::{blockchain::provider::RethProvider, config::RethConfig};
use libsofl_utils::{
    config::Config,
    log::{config::LogConfig, error, info},
    solidity::caller::HighLevelCaller,
    sync::runtime::AsyncRuntime,
};
use proxyex_detector::{config::ProxyExDetectorConfig, entities};
use rayon::ThreadPoolBuilder;
use sea_orm::{
    sea_query::{Expr, OnConflict, Query},
    Condition, DbErr, EntityTrait, PaginatorTrait, QueryFilter,
};

use crate::initialize_extractor::InitializeExtractor;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,

    #[arg(short, long, default_value = "1")]
    jobs: usize,

    #[arg(
        short = 'k',
        long,
        default_value = "proxyex_detector_public_initialize.csv"
    )]
    initialize_knowledge: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), DbErr> {
    let args = Cli::parse();
    let cfg = ProxyExDetectorConfig::must_load();
    let mut log_cfg = LogConfig::load_or(Default::default()).unwrap();
    log_cfg.console_level = args.log_level.clone();
    log_cfg.init();

    let p = RethConfig::must_load().bc_provider().unwrap();
    let p = Arc::new(p);

    let knowledge = load_initialize_knowledge(&args.initialize_knowledge).unwrap();
    let knowledge = Arc::new(knowledge);

    // collect_all(args, cfg, p).await
    frontrun_all(args, cfg, p, knowledge).await
}

fn load_initialize_knowledge(path: &str) -> Result<Vec<(Bytes, Bytes)>, std::io::Error> {
    let mut knowledge = Vec::new();
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        let mut iter = line.split(',');
        let sighash = iter.next().unwrap().to_string().cvt();
        let input = iter.next().unwrap().to_string().cvt();
        knowledge.push((sighash, input));
    }
    Ok(knowledge)
}

async fn frontrun_all(
    args: Cli,
    cfg: ProxyExDetectorConfig,
    p: Arc<RethProvider>,
    knowledge: Arc<Vec<(Bytes, Bytes)>>,
) -> Result<(), DbErr> {
    let (task_tx, task_rx) = channel::bounded::<(Address, TxHash)>(1000);
    let (result_tx, result_rx) = channel::bounded::<entities::initialize::Model>(1000);

    let pool = ThreadPoolBuilder::default()
        .num_threads(args.jobs)
        .build()
        .unwrap();

    let cloned_cfg = cfg.clone();
    let result_thread = thread::spawn(move || {
        let rt = AsyncRuntime::new();
        let db = rt.block_on(cloned_cfg.db()).unwrap();
        let mut cache: Vec<entities::initialize::ActiveModel> = Vec::new();
        loop {
            match result_rx.recv() {
                Ok(proxy) => {
                    cache.push(proxy.into());
                }
                Err(_) => {
                    let count = cache.len();
                    let task = entities::initialize::Entity::insert_many(cache.drain(..))
                        .on_conflict(
                            OnConflict::column(entities::initialize::Column::Proxy)
                                .update_column(entities::initialize::Column::Uninitialized)
                                .to_owned(),
                        )
                        .exec(&db);
                    match rt.block_on(task) {
                        Ok(_) => {
                            info!("{} contracts saved", count);
                        }
                        Err(e) => {
                            error!("save failed: {}", e);
                        }
                    }
                    break;
                }
            }
            if cache.len() >= 1 {
                let count = cache.len();
                let task = entities::initialize::Entity::insert_many(cache.drain(..))
                    .on_conflict(
                        OnConflict::column(entities::initialize::Column::Proxy)
                            .update_column(entities::initialize::Column::Uninitialized)
                            .update_column(entities::initialize::Column::FrontrunInput)
                            .to_owned(),
                    )
                    .exec(&db);
                match rt.block_on(task) {
                    Ok(_) => {
                        info!("{} contracts saved", count);
                    }
                    Err(e) => {
                        error!("save failed: {}", e);
                    }
                }
            }
        }
    });

    let wg = WaitGroup::new();
    let finished = Arc::new(AtomicI32::new(0));
    for _ in 0..args.jobs {
        let wg = wg.clone();
        let task_rx = task_rx.clone();
        let result_tx = result_tx.clone();
        let p = p.clone();
        let finished = finished.clone();
        let cfg = cfg.clone();
        let knowledge = knowledge.clone();
        pool.spawn(move || {
            let rt = AsyncRuntime::new();
            let db = rt.block_on(cfg.db()).unwrap();
            loop {
                match task_rx.recv() {
                    Ok((contract, creation_tx)) => {
                        let task = async {
                            entities::initialize::Entity::find_by_id(
                                contract.to_string().to_lowercase(),
                            )
                            .one(&db)
                            .await
                        };
                        let mut proxy = match rt.block_on(task) {
                            Ok(p) => p.unwrap(),
                            Err(_) => {
                                break;
                            }
                        };
                        let uninitialized = check_uninitialized(
                            p.clone(),
                            knowledge.clone(),
                            contract,
                            creation_tx,
                        )
                        .unwrap();
                        proxy.uninitialized = Some(uninitialized.is_some());
                        proxy.frontrun_input = match uninitialized {
                            Some(input) => Some(input.to_string().to_lowercase()),
                            None => None,
                        };
                        result_tx.send(proxy).unwrap();

                        finished.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        info!(
                            finished = finished.load(std::sync::atomic::Ordering::SeqCst),
                            "finished proxy"
                        );
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
            drop(wg);
        });
    }

    let db = cfg.db().await.unwrap();
    let mut paginator = entities::creation::Entity::find()
        .filter(
            Condition::all().add(Expr::exists(
                Query::select()
                    .from(entities::initialize::Entity)
                    .and_where(
                        Expr::col((
                            entities::creation::Entity,
                            entities::creation::Column::Proxy,
                        ))
                        .equals(entities::initialize::Column::Proxy),
                    )
                    .and_where(Expr::col(entities::initialize::Column::Uninitialized).is_null())
                    .take(),
            )),
        )
        .paginate(&db, 10000);
    while let Some(proxies) = paginator.fetch_and_next().await.unwrap() {
        for proxy in proxies {
            task_tx
                .send((proxy.proxy.cvt(), proxy.creation_tx.cvt()))
                .unwrap();
        }
    }

    info!("Waiting for all tasks to finish");
    drop(task_tx);
    wg.wait();

    info!("Waiting for result thread to finish");
    drop(result_tx);
    result_thread.join().unwrap();

    Ok(())
}

#[allow(unused)]
async fn collect_all(
    args: Cli,
    cfg: ProxyExDetectorConfig,
    p: Arc<RethProvider>,
) -> Result<(), DbErr> {
    let (task_tx, task_rx) = channel::bounded::<Address>(1000);
    let (result_tx, result_rx) = channel::bounded::<(Address, Option<Bytes>, Option<Bytes>)>(1000);

    let pool = ThreadPoolBuilder::default()
        .num_threads(args.jobs)
        .build()
        .unwrap();

    let cloned_cfg = cfg.clone();
    let result_thread = thread::spawn(move || {
        let rt = AsyncRuntime::new();
        let db = rt.block_on(cloned_cfg.db()).unwrap();
        let mut cache: Vec<entities::initialize::ActiveModel> = Vec::new();
        loop {
            match result_rx.recv() {
                Ok((contract, sighash, input)) => {
                    cache.push(entities::initialize::ActiveModel {
                        proxy: sea_orm::ActiveValue::Set(contract.to_string().to_lowercase()),
                        sighash: sea_orm::ActiveValue::Set(
                            sighash.map(|b| b.to_string().to_lowercase()),
                        ),
                        initialize_input: sea_orm::ActiveValue::Set(
                            input.map(|b| b.to_string().to_lowercase()),
                        ),
                        uninitialized: sea_orm::ActiveValue::Set(None),
                        frontrun_input: sea_orm::ActiveValue::Set(None),
                    });
                }
                Err(_) => {
                    let count = cache.len();
                    let task = entities::initialize::Entity::insert_many(cache.drain(..))
                        .on_conflict(
                            OnConflict::column(entities::initialize::Column::Proxy)
                                .do_nothing()
                                .to_owned(),
                        )
                        .exec(&db);
                    match rt.block_on(task) {
                        Ok(_) => {
                            info!("{} contracts saved", count);
                        }
                        Err(e) => {
                            error!("save failed: {}", e);
                        }
                    }
                    break;
                }
            }
            if cache.len() >= args.jobs {
                let count = cache.len();
                let task = entities::initialize::Entity::insert_many(cache.drain(..))
                    .on_conflict(
                        OnConflict::column(entities::initialize::Column::Proxy)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec(&db);
                match rt.block_on(task) {
                    Ok(_) => {
                        info!("{} contracts saved", count);
                    }
                    Err(e) => {
                        error!("save failed: {}", e);
                    }
                }
            }
        }
    });

    let wg = WaitGroup::new();
    let finished = Arc::new(AtomicI32::new(0));
    for _ in 0..args.jobs {
        let wg = wg.clone();
        let task_rx = task_rx.clone();
        let result_tx = result_tx.clone();
        let p = p.clone();
        let finished = finished.clone();
        let cfg = cfg.clone();
        pool.spawn(move || {
            let rt = AsyncRuntime::new();
            let db = rt.block_on(cfg.db()).unwrap();
            loop {
                match task_rx.recv() {
                    Ok(contract) => {
                        let task = async {
                            entities::creation::Entity::find_by_id(
                                contract.to_string().to_lowercase(),
                            )
                            .one(&db)
                            .await
                        };
                        let creation_tx = match rt.block_on(task) {
                            Ok(creation) => creation.unwrap().creation_tx,
                            Err(_) => {
                                break;
                            }
                        }
                        .cvt();
                        let result =
                            collect_initialize_input(p.clone(), contract, creation_tx).unwrap();
                        if let Some(input) = result {
                            if input.len() < 4 {
                                result_tx.send((contract, None, None)).unwrap();
                            } else {
                                let sighash: Bytes = input[0..4].to_vec().cvt();
                                result_tx
                                    .send((contract, Some(sighash), Some(input)))
                                    .unwrap();
                            }
                        } else {
                            result_tx.send((contract, None, None)).unwrap();
                        }
                        finished.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        info!(
                            finished = finished.load(std::sync::atomic::Ordering::SeqCst),
                            "finished proxy"
                        );
                    }
                    Err(_) => {
                        break;
                    }
                }
            }
            drop(wg);
        });
    }

    let db = cfg.db().await.unwrap();
    let mut paginator = entities::proxy::Entity::find()
        .filter(
            Condition::all().add(
                Expr::exists(
                    Query::select()
                        .from(entities::initialize::Entity)
                        .and_where(
                            Expr::col((entities::proxy::Entity, entities::proxy::Column::Address))
                                .equals(entities::initialize::Column::Proxy),
                        )
                        .take(),
                )
                .not(),
            ),
        )
        .paginate(&db, 10000);
    while let Some(proxies) = paginator.fetch_and_next().await.unwrap() {
        for proxy in proxies {
            task_tx.send(proxy.address.cvt()).unwrap();
        }
    }

    info!("Waiting for all tasks to finish");
    drop(task_tx);
    wg.wait();

    info!("Waiting for result thread to finish");
    drop(result_tx);
    result_thread.join().unwrap();

    Ok(())
}

#[allow(unused)]
fn collect_initialize_input(
    p: Arc<RethProvider>,
    contract: Address,
    creation_tx: TxHash,
) -> Result<Option<Bytes>, SoflError> {
    let tx = p.tx(creation_tx.cvt())?;
    let pos = tx.position().unwrap();
    let mut state = p.bc_state_at(pos)?;
    let spec = TransitionSpecBuilder::default()
        .at_block(p.clone(), pos.block)
        .append_tx(tx)
        .build();
    let mut insp = InitializeExtractor::new(contract);
    state.transit(spec, &mut insp)?;
    Ok(insp.initialize_input)
}

/// Check if a contract is uninitialized after creation.
#[allow(unused)]
fn check_uninitialized(
    p: Arc<RethProvider>,
    knowledge: Arc<Vec<(Bytes, Bytes)>>,
    contract: Address,
    creation_tx: TxHash,
) -> Result<Option<Bytes>, SoflError> {
    let creation_tx = p.tx(creation_tx.cvt())?;
    let mut pos = creation_tx.position().unwrap();
    pos.shift(&p, 1).unwrap();
    let mut state = p.bc_state_at(pos)?;
    for (_, input) in knowledge.iter() {
        let mut insp = HasDelegateCallOrNot {
            contract,
            has_delegatecall: false,
            updated_contract: false,
        };
        let r = HighLevelCaller::default()
            .bypass_check()
            .at_block(p.clone(), pos.block)
            .simulate_call(&mut state, contract, input.to_owned(), None, &mut insp);
        if r.is_ok() && !insp.has_delegatecall && insp.updated_contract {
            return Ok(Some(input.to_owned()));
        }
    }
    Ok(None)
}

#[allow(unused)]
fn frontrun_call(
    p: Arc<RethProvider>,
    contract: Address,
    creation_tx: TxHash,
    input: Bytes,
) -> Result<bool, SoflError> {
    let creation_tx = p.tx(creation_tx.cvt())?;
    let mut pos = creation_tx.position().unwrap();
    pos.shift(&p, 1).unwrap();
    let mut state = p.bc_state_at(pos)?;
    let mut insp = HasDelegateCallOrNot {
        contract,
        has_delegatecall: false,
        updated_contract: false,
    };
    let r = HighLevelCaller::default()
        .bypass_check()
        .at_block(p.clone(), pos.block)
        .call(&mut state, contract, input, None, &mut insp);
    if r.is_ok() {
        return Ok(true && !insp.has_delegatecall);
    } else {
        return Ok(false);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use libsofl_core::{
        conversion::ConvertTo,
        engine::types::{Address, Bytes, TxHash},
    };
    use libsofl_reth::config::RethConfig;
    use libsofl_utils::config::Config;

    #[test]
    fn test_frontrun_wormhole_uninitialize_bug() {
        let p = RethConfig::must_load().bc_provider().unwrap();
        let p = Arc::new(p);
        let implementation: Address = "0x736d2a394f7810c17b3c6fed017d5bc7d60c077d".cvt();
        let input: Bytes = "0xf6079017000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000".cvt(); // call initialize() function
        let creation_tx: TxHash =
            "0xa52ffec49d2dba0bb04ae9c95dd3876232b316fdef4fe5ec1dd7327b7bdfd4c3".cvt();
        let success = super::frontrun_call(p.clone(), implementation, creation_tx, input).unwrap();
        assert!(success);
    }

    #[test]
    #[ignore = "will fail"]
    fn test_frontrun_initialization() {
        let p = RethConfig::must_load().bc_provider().unwrap();
        let p = Arc::new(p);
        let proxy: Address = "0x7d2768de32b0b80b7a3454c06bdac94a69ddc7a9".cvt();
        let input: Bytes = "0xd1f5789400000000000000000000000086765dde9304bea32f65330d266155c4fa0c4f0400000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000000".cvt(); // call initialize() function
        let creation_tx: TxHash =
            "0x7d77cc7523a491fa670bfefa0a386ab036b6511d6d9fa6c2cf5c07b349dc9d3a".cvt();
        let success = super::frontrun_call(p.clone(), proxy, creation_tx, input).unwrap();
        assert!(!success);
    }
}
