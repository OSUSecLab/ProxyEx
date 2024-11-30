mod generator;

use std::sync::{Arc, Mutex};

use clap::Parser;
use crossbeam::channel;
use generator::{DBIterator, Item};
use libsofl_core::{
    conversion::ConvertTo,
    engine::types::{Address, Bytecode},
};
use libsofl_reth::{
    blockchain::provider::{RethProvider, StateProviderFactory},
    config::RethConfig,
};
use libsofl_utils::{
    config::Config,
    log::{debug, error, info},
    sync::runtime::AsyncRuntime,
};
use proxyex_detector::{
    config::ProxyExDetectorConfig,
    entities,
    replaced_replay::{check_regression, regression_one_tx, RegressionIssue},
};
use rayon::ThreadPoolBuilder;
use sea_orm::{sea_query::OnConflict, ColumnTrait, Condition, DbErr, EntityTrait, QueryFilter};
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,

    #[arg(short, long, default_value = "1")]
    jobs: usize,

    proxies: Option<String>,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), DbErr> {
    let args = Cli::parse();
    // prepare logger
    let indicatif_layer = IndicatifLayer::new();
    let log_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(args.log_level.clone()))
        .expect("failed to create console logger filter");
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(indicatif_layer.get_stderr_writer())
                .with_target(false)
                .with_filter(log_filter),
        )
        .with(indicatif_layer)
        .init();

    let only_proxies: Option<Vec<Address>> = args.proxies.map(|s| {
        s.split(',')
            .map(|ss| ConvertTo::<Address>::cvt(&ss))
            .collect()
    });
    let cfg = ProxyExDetectorConfig::must_load();
    let provider = RethConfig::must_load().bc_provider().unwrap();
    let provider = Arc::new(provider);

    analyze_all(cfg, provider, args.jobs, only_proxies).await;

    Ok(())
}

async fn analyze_all(
    cfg: ProxyExDetectorConfig,
    p: Arc<RethProvider>,
    jobs: usize,
    only_proxies: Option<Vec<Address>>,
) {
    let regression_mu = Mutex::new(());
    let regression_mu = Arc::new(regression_mu);

    let db = cfg.db().await.unwrap();
    let mut generator = DBIterator::new(db, jobs * 2, only_proxies, regression_mu.clone());

    let pool = ThreadPoolBuilder::new().num_threads(jobs).build().unwrap();
    let (issue_tx, issue_rx) = channel::bounded::<Vec<RegressionIssue>>(jobs * 2);

    let db = cfg.db().await.unwrap();
    let r_mu = regression_mu.clone();
    pool.spawn(move || {
        let rt = AsyncRuntime::new();
        let mut issues_buf = Vec::new();
        loop {
            let issues = match issue_rx.recv() {
                Ok(x) => x,
                Err(_) => break,
            };
            debug!(count = issues.len(), "received regression issues");
            issues_buf.extend(issues);
            if issues_buf.len() >= jobs {
                let lck = r_mu.lock().unwrap();
                debug!(count = issues_buf.len(), "inserting regression issues");
                let len = issues_buf.len();
                let task = async {
                    entities::regression::Entity::insert_many(
                        issues_buf
                            .into_iter()
                            .map(|s| s.into())
                            .collect::<Vec<entities::regression::ActiveModel>>(),
                    )
                    .on_conflict(OnConflict::new().do_nothing().to_owned())
                    .exec(&db)
                    .await
                };
                let r = rt.block_on(task);
                match r {
                    Ok(_) => {
                        info!(count = len, "inserted regression issues")
                    }
                    Err(e) => {
                        if e != DbErr::RecordNotInserted {
                            error!(e = ?e, "failed to insert regression issues")
                        } else {
                            debug!(count = len, "duplicate regression issues")
                        }
                    }
                }
                drop(lck);
                issues_buf = Vec::new();
            } else {
                debug!(count = issues_buf.len(), "buffering regression issues");
            }
        }
    });

    let (task_tx, task_rx) = channel::bounded::<Item>(jobs * 2);
    let finished = Mutex::new(0);
    let finished = Arc::new(finished);
    for _ in 0..jobs {
        let task_rx = task_rx.clone();
        let p = p.clone();
        let issue_tx = issue_tx.clone();
        let finished = finished.clone();
        let cfg = cfg.clone();
        pool.spawn(move || {
            let rt = AsyncRuntime::new();
            let db = rt.block_on(cfg.db()).unwrap();
            loop {
                let (proxy, implementation, blk, tx_hash) = match task_rx.recv() {
                    Ok(a) => a,
                    Err(_) => return,
                };
                debug!(proxy = proxy.to_string().to_lowercase(), tx = tx_hash.to_string(), "geting regression versions");
                let task = async {
                    entities::version::Entity::find()
                        .filter(
                            Condition::all().add(
                                entities::version::Column::Proxy.eq(proxy.to_string().to_lowercase()),
                            ), // .add(entities::version::Column::MinBlock.gt(blk)),
                        )
                        // .order_by_asc(entities::version::Column::MinBlock)
                        .all(&db)
                        .await
                        .unwrap()
                };
                let alt_versions: Vec<entities::version::Model> = rt.block_on(task);
                let alts: Vec<(Address, Bytecode)> = alt_versions
                .into_iter()
                .filter(|m| m.min_block > blk)
                .map(|m| {
                    let code =
                        p.bp.state_by_block_number_or_tag((m.min_block as u64).into())
                            .unwrap()
                            .account_code(m.implementation.cvt())
                            .unwrap()
                            .unwrap_or_default()
                            .bytes()
                            .to_owned();
                    (m.implementation.cvt(), code.cvt())
                })
                .collect();
                let alt_count = alts.len();
                let (original_insp, alt_insps) = match regression_one_tx(
                    p.clone(),
                    proxy,
                    implementation,
                    alts,
                    tx_hash,
                ) {
                    Ok(x) => x,
                    Err(e) => {
                        error!(e = ?e, proxy = proxy.to_string().to_lowercase(), tx = tx_hash.to_string(), alts = alt_count, "failed to regression test on tx");
                        return;
                    }
                };
                assert_eq!(alt_insps.len(), alt_count);
                let rs = match check_regression(original_insp, alt_insps, tx_hash) {
                    Ok(x) => x,
                    Err(e) => {
                        error!(e = ?e, proxy = proxy.to_string().to_lowercase(), tx = tx_hash.to_string(), "failed to check regression");
                        return;
                    }
                };
                assert_eq!(rs.len(), alt_count);
                issue_tx.send(rs).unwrap();
                let mut finished = finished.lock().unwrap();
                *finished += 1;
                let finished = *finished;
                info!(proxy = proxy.to_string().to_lowercase(), tx = tx_hash.to_string(), alts = alt_count, finished, "regression tested");
                // let mut c = count.lock().unwrap();
                // *c += 1;
                // progress_span.pb_set_message(format!("Analyzed {}", *c).as_str());
            }
        });
    }

    loop {
        let (proxy, implementation, blk, tx_hash) = match generator.next_async().await {
            Some(a) => a,
            None => break,
        };

        task_tx.send((proxy, implementation, blk, tx_hash)).unwrap();
    }
    drop(task_rx);
    info!("All tasks sent");
}
