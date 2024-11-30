use std::sync::Arc;

use clap::{command, Parser, ValueEnum};
use generator::DBIterator;
use libsofl_reth::config::RethConfig;
use libsofl_utils::{
    config::Config,
    log::{error, info},
};
use libsofl_utils::{log::debug, sync::runtime::AsyncRuntime};
use proxyex_detector::original_replay::OriginalReplayScheduler;
use proxyex_detector::{config::ProxyExDetectorConfig, entities};
use sea_orm::{sea_query::OnConflict, ActiveValue, DbErr, EntityTrait};
use tracing_indicatif::IndicatifLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use crate::generator::{build_from_all, build_from_proxies};

mod generator;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,

    #[arg(short, long, value_enum, default_value = "original")]
    mode: Mode,

    #[arg(short, long, default_value = "1")]
    jobs: usize,

    /// One single proxy data entry or a list of proxy addresses
    proxy_data: Option<String>,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
enum Mode {
    Original,
    Replaced,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), DbErr> {
    let args = Cli::parse();
    info!("Replay started: {:?}", args);

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

    let cfg = ProxyExDetectorConfig::must_load();

    let generator = match args.proxy_data.clone() {
        Some(proxy_data) => {
            let db = cfg.db().await?;
            build_from_proxies(db, proxy_data.as_str()).await
        }
        None => {
            let db = cfg.db().await?;
            build_from_all(db).await
        }
    };
    match args.mode {
        Mode::Original => original_replay(args, generator).await,
        Mode::Replaced => todo!(),
    };
    Ok(())
}

async fn original_replay(args: Cli, mut proxy_data: DBIterator) {
    let provider = RethConfig::must_load().bc_provider().unwrap();
    let provider = Arc::new(provider);
    let (proxy_result_tx, proxy_result_rx) = crossbeam::channel::bounded(args.jobs);
    let scheduler = OriginalReplayScheduler::new(provider, args.jobs, proxy_result_tx);

    // collector thread received the aggregated proxy analysis result from the scheduler
    let collector_thread = std::thread::spawn(move || {
        info!("Result collector thread started");
        let rt = AsyncRuntime::new();
        let task = async {
            let cfg = proxyex_detector::config::ProxyExDetectorConfig::load_or(Default::default())
                .expect("load config failed");
            let db = match cfg.db().await {
                Ok(db) => db,
                Err(e) => {
                    error!(error = ?e, "Failed to connect to database");
                    return;
                }
            };
            let mut finished = 0;
            loop {
                debug!("Waiting for result");
                let result = match proxy_result_rx.recv() {
                    Ok(r) => r,
                    Err(_) => {
                        info!("Result channel closed, result collector thread exit");
                        break;
                    }
                };
                match result {
                    Ok(r) => {
                        finished += 1;
                        info!(
                            proxy = r.proxy.to_string(),
                            finished = finished,
                            "Replay finished"
                        );
                        let result: entities::collision::ActiveModel = r.into();
                        let r = entities::collision::Entity::insert(result)
                            .on_conflict(
                                OnConflict::column(entities::collision::Column::Proxy)
                                    .do_nothing()
                                    .to_owned(),
                            )
                            .exec(&db)
                            .await;
                        match r {
                            Ok(_) => {}
                            Err(e) => {
                                if e != DbErr::RecordNotInserted {
                                    error!(error = ?e, "Failed to save replay result");
                                }
                            }
                        }
                    }
                    Err(error) => {
                        error!(error = ?error, "Replay error");
                        let error = entities::error::ActiveModel {
                            proxy: ActiveValue::Set(error.proxy.to_string()),
                            msg: ActiveValue::Set(error.msg),
                            ..Default::default()
                        };
                        entities::error::Entity::insert(error)
                            .exec(&db)
                            .await
                            .expect("Failed to save error");
                    }
                };
            }
        };
        rt.block_on(task)
    });

    loop {
        if let Some(data) = proxy_data.next_async().await {
            debug!("Feed proxy invocation: {:?}", data);
            scheduler.feed_proxy_invocation_in_order(data.0, data.1, data.2, data.3, data.4);
        } else {
            break;
        }
    }

    info!("Waiting for scheduler to finish");
    scheduler.close();
    collector_thread.join().unwrap();
}
