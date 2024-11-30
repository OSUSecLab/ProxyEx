mod inspector;

use std::{sync::Arc, thread, time::Duration};

use clap::{command, Parser};
use crossbeam::{channel, sync::WaitGroup};
use inspector::ImplInspector;
use libsofl_core::{
    blockchain::{provider::BcStateProvider, tx_position::TxPosition},
    conversion::ConvertTo,
    engine::types::{Address, Bytes, Database, U256},
    error::SoflError,
};
use libsofl_reth::{blockchain::provider::RethProvider, config::RethConfig};
use libsofl_utils::{
    config::Config,
    log::{config::LogConfig, info},
    solidity::caller::HighLevelCaller,
    sync::runtime::AsyncRuntime,
};
use proxyex_detector::{config::ProxyExDetectorConfig, entities};
use rayon::ThreadPoolBuilder;
use sea_orm::{
    sea_query::{Expr, OnConflict, Query},
    ColumnTrait, DbErr, EntityTrait, PaginatorTrait, QueryFilter,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,

    #[arg(short, long, default_value = "1")]
    jobs: usize,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), DbErr> {
    let args = Cli::parse();
    let cfg = ProxyExDetectorConfig::must_load();
    let mut log_cfg = LogConfig::load_or(Default::default()).unwrap();
    log_cfg.console_level = args.log_level.clone();
    log_cfg.init();

    let p = RethConfig::must_load().bc_provider().unwrap();
    let p = Arc::new(p);

    check_all(args, cfg, p).await
}

async fn check_all(
    args: Cli,
    cfg: ProxyExDetectorConfig,
    p: Arc<RethProvider>,
) -> Result<(), DbErr> {
    let (proxy_tx, proxy_rx) = channel::bounded::<entities::proxy::Model>(args.jobs);
    let (result_tx, result_rx) =
        channel::bounded::<(Address, Vec<(Address, Address, i64)>, Duration)>(args.jobs);

    let cloned_cfg = cfg.clone();
    let result_thread = thread::spawn(move || {
        let rt = AsyncRuntime::new();
        let db = rt.block_on(cloned_cfg.db()).unwrap();
        loop {
            let (proxy, mismatched_impls, time) = match result_rx.recv() {
                Ok(v) => v,
                Err(_) => break,
            };
            let task = async {
                let fake = entities::fake_loose::ActiveModel {
                    proxy: sea_orm::ActiveValue::Set(proxy.to_string().to_lowercase()),
                    problematic: sea_orm::ActiveValue::Set(mismatched_impls.len() > 0),
                    mismatched_impls: sea_orm::ActiveValue::Set(
                        serde_json::to_value(mismatched_impls).unwrap(),
                    ),
                    total_time: sea_orm::ActiveValue::Set(time.as_nanos() as i64),
                };
                entities::fake_loose::Entity::insert(fake)
                    .on_conflict(
                        OnConflict::column(entities::fake_loose::Column::Proxy)
                            .do_nothing()
                            .to_owned(),
                    )
                    .exec(&db)
                    .await
            };
            match rt.block_on(task) {
                Ok(_) => {}
                Err(e) => {
                    if e != DbErr::RecordNotInserted {
                        panic!("{:?}", e);
                    }
                }
            };
        }
    });

    let pool = ThreadPoolBuilder::new()
        .num_threads(args.jobs)
        .build()
        .unwrap();

    let wg = WaitGroup::new();
    for _ in 0..args.jobs {
        let proxy_rx = proxy_rx.clone();
        let result_tx = result_tx.clone();
        let p = p.clone();
        let wg = wg.clone();
        let cfg = cfg.clone();
        pool.spawn(move || {
            let rt = AsyncRuntime::new();
            let db = rt.block_on(cfg.db()).unwrap();
            loop {
                let proxy = match proxy_rx.recv() {
                    Ok(v) => v,
                    Err(_) => break,
                };
                info!(
                    proxy = proxy.address.to_string().to_lowercase(),
                    "Checking proxy"
                );
                let mut mismatched_impls = Vec::new();
                let time_elapsed;
                if proxy.invocation_count > 0 {
                    let task = async {
                        entities::version::Entity::find()
                            .filter(
                                entities::version::Column::Proxy
                                    .eq(proxy.address.to_string().to_lowercase()),
                            )
                            .all(&db)
                            .await
                    };
                    let versions = rt.block_on(task).unwrap();
                    let start_at = std::time::Instant::now();
                    for version in versions {
                        let impl_ =
                            check_impl_slot(p.clone(), proxy.address.cvt(), version.min_block + 1)
                                .unwrap();
                        let version_impl: Address = version.implementation.cvt();
                        if impl_ != version_impl {
                            mismatched_impls.push((impl_, version_impl, version.min_block + 1));
                        }
                    }
                    time_elapsed = start_at.elapsed();
                } else {
                    let task = async {
                        entities::creation::Entity::find()
                            .filter(
                                entities::creation::Column::Proxy
                                    .eq(proxy.address.to_string().to_lowercase()),
                            )
                            .one(&db)
                            .await
                    };
                    let creation = rt.block_on(task).unwrap().unwrap();
                    let start_at = std::time::Instant::now();
                    let impl_ = check_impl_slot(
                        p.clone(),
                        proxy.address.cvt(),
                        creation.creation_block + 1,
                    )
                    .unwrap();
                    let actual_impl = check_actual_impl(
                        p.clone(),
                        proxy.address.cvt(),
                        creation.creation_block + 1,
                    )
                    .unwrap();
                    if let Some(actual_impl) = actual_impl {
                        if actual_impl != impl_ {
                            mismatched_impls.push((impl_, actual_impl, 0));
                        }
                    }
                    time_elapsed = start_at.elapsed();
                }

                result_tx
                    .send((proxy.address.cvt(), mismatched_impls, time_elapsed))
                    .unwrap();
            }
            drop(wg);
        });
    }

    let db = cfg.db().await.unwrap();
    let mut proxies_paginator = entities::proxy::Entity::find()
        .filter(
            Expr::exists(
                Query::select()
                    .from(entities::fake_loose::Entity)
                    .and_where(
                        Expr::col((entities::fake_loose::Entity, entities::fake_loose::Column::Proxy))
                            .equals((entities::proxy::Entity, entities::proxy::Column::Address)),
                    )
                    .take(),
            )
            .not(),
        )
        .paginate(&db, 1000);
    while let Some(proxies) = proxies_paginator.fetch_and_next().await? {
        for proxy in proxies {
            proxy_tx.send(proxy).unwrap();
        }
    }

    drop(proxy_tx);
    info!("Waiting for all tasks to finish");
    wg.wait();

    drop(result_tx);
    info!("Waiting for result thread to finish");
    result_thread.join().unwrap();

    Ok(())
}

fn check_actual_impl(
    p: Arc<RethProvider>,
    proxy: Address,
    blk: i64,
) -> Result<Option<Address>, SoflError> {
    let mut state = p.bc_state_at(TxPosition::new(blk as u64, 0u64))?;
    let mut insp = ImplInspector {
        proxy,
        implementation: None,
    };
    let inputs: Bytes = "0x8da5cb5b".cvt();
    let _ = HighLevelCaller::default()
        .bypass_check()
        .at_block(p.clone(), blk as u64)
        .call(&mut state, proxy, inputs, None, &mut insp);
    Ok(insp.implementation)
}

fn check_impl_slot(p: Arc<RethProvider>, proxy: Address, blk: i64) -> Result<Address, SoflError> {
    let mut state = p.bc_state_at(TxPosition::new(blk as u64, 0u64))?;

    // EIP-1882
    let slot: U256 = "0xc5f16f0fcc639fa48a6947836d9850f504798523bf8c9a3a87d5876cf622bcf7".cvt();
    let value = state.storage(proxy, slot).unwrap();
    if value != U256::ZERO {
        return Ok(value.cvt());
    }

    // EIP-1967
    let slot: U256 = "0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc".cvt();
    let value = state.storage(proxy, slot).unwrap();
    if value != U256::ZERO {
        return Ok(value.cvt());
    }

    Ok(Address::ZERO)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use libsofl_core::{
        blockchain::{provider::BcProvider, transaction::Tx},
        conversion::ConvertTo,
        engine::types::{Address, BlockHashOrNumber, TxHash},
    };
    use libsofl_reth::config::RethConfig;
    use libsofl_utils::config::Config;

    #[test]
    fn test_fake_proxy() {
        let proxy: Address = "0x407f5490cfa4cba715cb93645c988b504fcf0331".cvt();
        let slot_impl: Address = "0xc1e97d3fc2810577289ee35e895a4f0e59481700".cvt();
        let actual_impl: Address = "0x4674f9cf8fce3e9ff332015a0f0859baa60c2ded".cvt();
        let tx: TxHash = "0x1664a7b7cbbf5abf3647082037a808a7cda3468557f88141ca5fdaa3dab61354".cvt();
        let p = RethConfig::must_load().bc_provider().unwrap();
        let p = Arc::new(p);
        let tx = p.tx(tx.cvt()).unwrap();
        let blk = match tx.position().unwrap().block {
            BlockHashOrNumber::Number(n) => n,
            _ => panic!(),
        };
        let impl_ = super::check_impl_slot(p.clone(), proxy, blk as i64).unwrap();
        assert_eq!(impl_, slot_impl);
        assert_ne!(impl_, actual_impl);
    }

    #[test]
    fn test_get_actual_impl() {
        let proxy: Address = "0x565d27b66e3e0159f2e19c5f1e0d76f455434347".cvt();
        let blk = 14936510i64;
        let p = RethConfig::must_load().bc_provider().unwrap();
        let p = Arc::new(p);
        let impl_ = super::check_actual_impl(p.clone(), proxy, blk).unwrap();
        assert_eq!(
            impl_.unwrap().to_string(),
            "0x425Dbc4951c72F5F0562C928537805ec053EC780"
        );
    }

    #[test]
    fn test_get_actual_impl2() {
        let proxy: Address = "0x0ba45a8b5d5575935b8158a88c631e9f9c95a2e5".cvt();
        let blk = 18000000i64;
        let p = RethConfig::must_load().bc_provider().unwrap();
        let p = Arc::new(p);
        let impl_ = super::check_actual_impl(p.clone(), proxy, blk).unwrap();
        assert_eq!(
            impl_.unwrap().to_string(),
            "0x687924f76f8A6768da69db3775003f4De7F7357c"
        );
    }
}
