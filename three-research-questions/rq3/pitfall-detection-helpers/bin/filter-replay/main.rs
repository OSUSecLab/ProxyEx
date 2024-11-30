use std::{collections::HashSet, sync::Arc};

use clap::{arg, command, Parser};
use crossbeam::{channel, sync::WaitGroup};
use libsofl_core::{
    blockchain::{
        provider::{BcProvider, BcStateProvider},
        transaction::Tx,
    },
    conversion::ConvertTo,
    engine::{
        state::BcState,
        transition::TransitionSpecBuilder,
        types::{Address, TxHash, U256},
    },
};
use libsofl_reth::{blockchain::provider::RethProvider, config::RethConfig};
use libsofl_utils::{
    config::Config,
    log::{config::LogConfig, info},
    sync::runtime::AsyncRuntime,
};
use proxyex_detector::{
    config::ProxyExDetectorConfig, entities, inspectors::collision::StorageAccessInspector,
};
use rayon::{
    iter::{IntoParallelRefIterator, ParallelIterator},
    ThreadPoolBuilder,
};
use sea_orm::{
    sea_query::{Expr, IntoCondition, Query},
    ColumnTrait, Condition, DatabaseConnection, DbErr, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, RelationTrait,
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
    let mut log_cfg = LogConfig::load_or(Default::default()).unwrap();
    log_cfg.console_level = args.log_level.clone();
    log_cfg.init();
    let provider = RethConfig::must_load().bc_provider().unwrap();
    let provider = Arc::new(provider);
    let cfg = ProxyExDetectorConfig::must_load();

    analyze_all(args, cfg, provider).await
}

async fn analyze_all(
    args: Cli,
    cfg: ProxyExDetectorConfig,
    provider: Arc<RethProvider>,
) -> Result<(), DbErr> {
    let pool = ThreadPoolBuilder::new()
        .num_threads(args.jobs + 1)
        .build()
        .unwrap();

    let (proxy_tx, proxy_rx) = channel::bounded::<(Address, i32)>(args.jobs);
    let (info_tx, info_rx) = channel::bounded::<(Address, Info)>(args.jobs);

    let cloned_cfg = cfg.clone();
    let result_thread = std::thread::spawn(move || {
        let rt = AsyncRuntime::new();
        let db = rt.block_on(cloned_cfg.db()).unwrap();
        let mut infos = Vec::new();
        loop {
            let (proxy, info) = match info_rx.recv() {
                Ok(t) => t,
                Err(_) => {
                    if infos.len() > 0 {
                        rt.block_on(save_infos(&db, &infos)).unwrap();
                    }
                    break;
                }
            };
            infos.push((proxy, info));
            if infos.len() > 0 {
                rt.block_on(save_infos(&db, &infos)).unwrap();
                infos.clear();
            }
        }
    });

    let finished = Arc::new(std::sync::atomic::AtomicI32::new(0));
    let wg = WaitGroup::new();
    for _ in 0..args.jobs {
        let cfg = cfg.clone();
        let proxy_rx = proxy_rx.clone();
        let info_tx = info_tx.clone();
        let provider = provider.clone();
        let finished = finished.clone();
        let wg = wg.clone();
        pool.spawn(move || {
            let rt = AsyncRuntime::new();
            let db = rt.block_on(cfg.db()).unwrap();
            loop {
                let (proxy, total) = match proxy_rx.recv() {
                    Ok(t) => t,
                    Err(_) => break,
                };
                let info = rt
                    .block_on(analyze_one(provider.clone(), &db, proxy, total))
                    .unwrap();
                finished.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                info!(
                    proxy = proxy.to_string().to_lowercase(),
                    finished = finished.load(std::sync::atomic::Ordering::SeqCst),
                    "finished"
                );
                match info_tx.send((proxy, info)) {
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            drop(wg);
        });
    }

    let db = cfg.db().await?;
    let mut proxy_paginater = entities::replay::Entity::find()
        .select_only()
        .column(entities::replay::Column::Proxy)
        .column(entities::proxy::Column::InvocationCount)
        .join(
            sea_orm::JoinType::InnerJoin,
            entities::proxy::Relation::Replay
                .def()
                .rev()
                .on_condition(|left, right| {
                    Expr::col((left, entities::replay::Column::Proxy))
                        .equals((right, entities::proxy::Column::Address))
                        .into_condition()
                }),
        )
        .filter(
            Condition::all()
                .add(entities::replay::Column::Problematic.eq(true))
                .add(
                    Expr::exists(
                        Query::select()
                            .from(entities::filtered_replay::Entity)
                            .and_where(
                                Expr::col((
                                    entities::replay::Entity,
                                    entities::replay::Column::Proxy,
                                ))
                                .equals((
                                    entities::filtered_replay::Entity,
                                    entities::filtered_replay::Column::Proxy,
                                )),
                            )
                            .take(),
                    )
                    .not(),
                ),
        )
        .into_tuple::<(String, i32)>()
        .paginate(&db, 500);
    while let Some(proxies) = proxy_paginater.fetch_and_next().await? {
        for (proxy, total) in proxies {
            let proxy: Address = proxy.cvt();
            proxy_tx.send((proxy, total)).unwrap();
        }
    }

    drop(proxy_tx);
    info!("waiting for all jobs to finish");
    wg.wait();

    drop(info_tx);
    info!("waiting for result thread to finish");
    result_thread.join().unwrap();

    Ok(())
}

async fn save_infos(db: &DatabaseConnection, infos: &Vec<(Address, Info)>) -> Result<(), DbErr> {
    let mut entities = Vec::new();
    for (proxy, info) in infos {
        let entity = entities::filtered_replay::ActiveModel {
            proxy: sea_orm::ActiveValue::Set(proxy.to_string().to_lowercase()),
            conflict_slots: sea_orm::ActiveValue::Set(
                serde_json::to_value(&info.conflict_slots).unwrap(),
            ),
            proxy_sstores: sea_orm::ActiveValue::Set(
                serde_json::to_value(&info.proxy_sstores).unwrap(),
            ),
            implementation_sstores: sea_orm::ActiveValue::Set(
                serde_json::to_value(&info.impl_sstores).unwrap(),
            ),
        };
        entities.push(entity);
    }
    entities::filtered_replay::Entity::insert_many(entities)
        .exec(db)
        .await?;
    info!(count = infos.len(), "saved proxy infos");
    Ok(())
}

struct Info {
    conflict_slots: HashSet<U256>,
    proxy_sstores: Vec<(TxHash, Vec<(U256, U256)>)>,
    impl_sstores: Vec<(TxHash, Address, Vec<(U256, U256)>)>,
}

async fn analyze_one(
    provider: Arc<RethProvider>,
    db: &DatabaseConnection,
    proxy: Address,
    total: i32,
) -> Result<Info, DbErr> {
    let replay = entities::replay::Entity::find()
        .filter(entities::replay::Column::Proxy.eq(proxy.to_string().to_lowercase()))
        .one(db)
        .await?
        .unwrap();
    let proxy_sstores =
        serde_json::from_value::<HashSet<(String, U256)>>(replay.proxy_sstores.clone()).unwrap();
    let impl_sstores =
        serde_json::from_value::<HashSet<(String, U256)>>(replay.implementation_sstores.clone())
            .unwrap();
    let conflicts = proxy_sstores
        .intersection(&impl_sstores)
        .collect::<HashSet<_>>();

    info!(
        proxy = proxy.to_string().to_lowercase(),
        invocations = total,
        "analyzing"
    );

    let mut all_proxy_sstores = Vec::new();
    let mut all_impl_sstores = Vec::new();
    let mut invocations_paginater = entities::invocation::Entity::find()
        .filter(entities::invocation::Column::Proxy.eq(proxy.to_string().to_lowercase()))
        .paginate(db, 64);
    while let Some(invocations) = invocations_paginater.fetch_and_next().await? {
        let sstores: Vec<(
            (TxHash, Vec<(U256, U256)>),
            (TxHash, Address, Vec<(U256, U256)>),
        )> = invocations
            .par_iter()
            .map(|inv| {
                let tx_hash: TxHash = inv.tx.cvt();
                let tx = provider.tx(tx_hash.cvt()).unwrap();
                let pos = tx.position().unwrap();
                let mut state = provider.bc_state_at(pos).unwrap();
                let spec = TransitionSpecBuilder::default()
                    .at_block(&provider, pos.block)
                    .append_tx(tx)
                    .build();
                let mut insp =
                    StorageAccessInspector::new(proxy, inv.implementation.cvt(), 0, 1, false);
                state.transit(spec, &mut insp).unwrap();
                let mut proxy_sstores = Vec::new();
                let mut impl_sstores = Vec::new();
                if !insp.proxy_created {
                    // we don't consider txs in which proxy is created
                    for proxy_sstore in insp.proxy_sstores {
                        let t = (proxy_sstore.0.to_string().to_lowercase(), proxy_sstore.1);
                        if conflicts.contains(&t) {
                            proxy_sstores.push((proxy_sstore.1, proxy_sstore.2));
                        }
                    }
                    for impl_sstore in insp.implementation_sstores {
                        let t = (impl_sstore.0.to_string().to_lowercase(), impl_sstore.1);
                        if conflicts.contains(&t) {
                            impl_sstores.push((impl_sstore.1, impl_sstore.2));
                        }
                    }
                }
                (
                    (tx_hash, proxy_sstores),
                    (tx_hash, inv.implementation.cvt(), impl_sstores),
                )
            })
            .collect();
        for sstore in sstores {
            if !sstore.0 .1.is_empty() {
                all_proxy_sstores.push(sstore.0);
            }
            if !sstore.1 .2.is_empty() {
                all_impl_sstores.push(sstore.1);
            }
        }
    }

    // re-calculate conflict slots
    let proxy_sstore_slots = all_proxy_sstores
        .iter()
        .map(|(_, sstores)| sstores.iter().map(|(slot, _)| slot).collect::<Vec<_>>())
        .flatten()
        .collect::<HashSet<_>>();
    let impl_sstore_slots = all_impl_sstores
        .iter()
        .map(|(_, _, sstores)| sstores.iter().map(|(slot, _)| slot).collect::<Vec<_>>())
        .flatten()
        .collect::<HashSet<_>>();
    let conflict_slots = proxy_sstore_slots
        .intersection(&impl_sstore_slots)
        .map(|slot| **slot)
        .collect::<HashSet<_>>();

    // shrink all_proxy_sstores and all_impl_sstores
    let mut new_all_proxy_sstores = Vec::new();
    let mut new_all_impl_sstores = Vec::new();
    for (tx_hash, proxy_sstores) in all_proxy_sstores.iter() {
        let mut new_proxy_sstores = Vec::new();
        for (slot, value) in proxy_sstores {
            if conflict_slots.contains(slot) {
                new_proxy_sstores.push((*slot, *value));
            }
        }
        if !new_proxy_sstores.is_empty() {
            new_all_proxy_sstores.push((*tx_hash, new_proxy_sstores));
        }
    }
    for (tx_hash, address, impl_sstores) in all_impl_sstores.iter() {
        let mut new_impl_sstores = Vec::new();
        for (slot, value) in impl_sstores {
            if conflict_slots.contains(slot) {
                new_impl_sstores.push((*slot, *value));
            }
        }
        if !new_impl_sstores.is_empty() {
            new_all_impl_sstores.push((*tx_hash, *address, new_impl_sstores));
        }
    }

    Ok(Info {
        conflict_slots,
        proxy_sstores: new_all_proxy_sstores,
        impl_sstores: new_all_impl_sstores,
    })
}
