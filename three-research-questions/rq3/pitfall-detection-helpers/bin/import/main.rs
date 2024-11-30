use std::io::{BufRead, BufReader};

use clap::{command, Parser};
use indicatif::ProgressStyle;
use libsofl_utils::{
    config::Config,
    log::{debug, error, info, info_span},
};
use proxyex_detector::{dataset::ProxyData, entities};
use sea_orm::{sea_query, ActiveValue, DbErr, EntityTrait, TransactionTrait};
use tracing_indicatif::{span_ext::IndicatifSpanExt, IndicatifLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'l', long, default_value = "info")]
    level: String,

    data: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    let args = Cli::parse();
    info!("Import started: {:?}", args);

    // prepare logger
    let indicatif_layer = IndicatifLayer::new();
    let log_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(args.level))
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

    // progress bar
    let progress_span = info_span!("importing");
    let pb_style = ProgressStyle::default_spinner();
    progress_span.pb_set_style(&pb_style);
    progress_span.pb_start();

    // prepare db
    let cfg = proxyex_detector::config::ProxyExDetectorConfig::load_or(Default::default())
        .expect("load config failed");
    let db = match cfg.db().await {
        Ok(db) => db,
        Err(e) => {
            error!(
                err = e.to_string().as_str(),
                "Failed to connect to database"
            );
            return;
        }
    };

    let generator = build_creation_data_generator(&args.data).unwrap();

    let mut finished_count = 0;
    for data in generator {
        let proxy_addr = data.proxy.clone();
        let txn = match db.begin().await {
            Ok(txn) => txn,
            Err(e) => {
                error!(err = e.to_string().as_str(), "Failed to start transaction");
                return;
            }
        };
        // any new proxy will not have invocations
        let proxy = entities::proxy::ActiveModel {
            address: ActiveValue::Set(data.proxy),
            invocation_count: ActiveValue::Set(0),
        };
        let r = entities::proxy::Entity::insert(proxy)
            .on_conflict(
                sea_query::OnConflict::column(entities::proxy::Column::Address)
                    .do_nothing()
                    .to_owned(),
            )
            .exec(&txn)
            .await;
        match r {
            Ok(_) => {}
            Err(e) => {
                if e != DbErr::RecordNotInserted {
                    error!(err = e.to_string().as_str(), "Failed to save proxy");
                    txn.rollback()
                        .await
                        .expect("Failed to rollback transaction");
                    return;
                }
            }
        };
        let creation = entities::creation::ActiveModel {
            proxy: ActiveValue::Set(proxy_addr.clone()),
            creation_tx: ActiveValue::Set(data.creation_tx),
            creation_block: ActiveValue::Set(data.creation_block),
            first_invocation_tx: ActiveValue::Set(data.first_invocation_tx),
            first_invocation_block: ActiveValue::Set(data.first_invocation_block),
        };
        let r = entities::creation::Entity::insert(creation)
            .on_conflict(
                sea_query::OnConflict::column(entities::creation::Column::Proxy)
                    .do_nothing()
                    .to_owned(),
            )
            .exec(&txn)
            .await;
        match r {
            Ok(_) => {}
            Err(e) => {
                if e != DbErr::RecordNotInserted {
                    error!(err = e.to_string().as_str(), "Failed to save creation");
                    txn.rollback()
                        .await
                        .expect("Failed to rollback transaction");
                    return;
                }
            }
        }

        match txn.commit().await {
            Ok(_) => debug!(proxy = proxy_addr, "Proxy saved"),
            Err(e) => {
                error!(err = e.to_string().as_str(), "Failed to commit transaction");
                return;
            }
        };

        finished_count += 1;
        progress_span.pb_set_message(format!("Imported {}", finished_count).as_str());
    }
}

struct CreationData {
    proxy: String,
    creation_tx: String,
    creation_block: i64,
    first_invocation_tx: Option<String>,
    first_invocation_block: Option<i64>,
}

fn build_creation_data_generator(
    file_path: &str,
) -> Result<Box<dyn Iterator<Item = CreationData>>, std::io::Error> {
    // try to parse proxy data as a file path
    let reader = BufReader::new(std::fs::File::open(file_path)?);
    let iter = reader
        .lines()
        .map(|line| {
            let line = line.unwrap();
            let mut segs = line.split(',');
            let proxy = segs.next().unwrap().to_owned().to_lowercase();
            let creation = segs.next().unwrap().to_owned();
            let mut creation_segs = creation.split(':');
            let creation_tx = creation_segs.next().unwrap().to_owned();
            let creation_block = creation_segs.next().unwrap().parse::<i64>().unwrap();
            let d = if let Some(first_invocation) = segs.next() {
                let mut first_invocation_segs = first_invocation.split(':');
                let first_invocation_tx = first_invocation_segs.next().unwrap().to_owned();
                let first_invocation_block = first_invocation_segs
                    .next()
                    .unwrap()
                    .parse::<i64>()
                    .unwrap();
                CreationData {
                    proxy,
                    creation_tx,
                    creation_block,
                    first_invocation_tx: Some(first_invocation_tx),
                    first_invocation_block: Some(first_invocation_block),
                }
            } else {
                CreationData {
                    proxy,
                    creation_tx,
                    creation_block,
                    first_invocation_tx: None,
                    first_invocation_block: None,
                }
            };
            d
        })
        .into_iter();
    Ok(Box::new(iter))
}

#[allow(unused)]
fn build_proxy_data_generator(
    proxy_data: &str,
) -> Result<Box<dyn Iterator<Item = ProxyData>>, std::io::Error> {
    match serde_json::from_str::<ProxyData>(proxy_data) {
        Ok(data) => {
            info!(data = proxy_data, "Proxy data is a single entry");
            let v = vec![data];
            Ok(Box::new(v.into_iter()))
        }
        Err(_) => {
            info!(
                path = proxy_data,
                "Considering proxy data as file containing multiple entries"
            );
            // try to parse proxy data as a file path
            let reader = BufReader::new(std::fs::File::open(proxy_data)?);
            let iter = reader
                .lines()
                .map(|line| {
                    let line = line.unwrap();
                    serde_json::from_str::<ProxyData>(&line).unwrap()
                })
                .into_iter();
            Ok(Box::new(iter))
        }
    }
}
