mod generator;

use clap::Parser;
use generator::DBIterator;
use indicatif::ProgressStyle;
use libsofl_utils::{
    config::Config,
    log::{error, info, info_span},
};
use proxyex_detector::{config::ProxyExDetectorConfig, entities};
use sea_orm::{
    sea_query::OnConflict, ActiveValue, ColumnTrait, DatabaseConnection, DbErr, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect,
};
use tracing_indicatif::{span_ext::IndicatifSpanExt, IndicatifLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,

    #[arg(short, long, default_value = "1")]
    jobs: usize,
}

// collect implementation versions of a proxy
#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), DbErr> {
    let args = Cli::parse();
    info!("Versioning started: {:?}", args);

    // prepare logger
    let indicatif_layer = IndicatifLayer::new();
    let log_filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(args.log_level))
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

    let db = cfg.db().await?;
    let iterator = DBIterator::new(db, args.jobs * 2);

    let db = cfg.db().await?;
    analyze_all(&db, iterator, args.jobs).await;
    Ok(())
}

async fn analyze_all(db: &DatabaseConnection, mut iterator: DBIterator, jobs: usize) {
    // progress bar
    let progress_span = info_span!("versioning");
    let pb_style = ProgressStyle::default_spinner();
    progress_span.pb_set_style(&pb_style);
    progress_span.pb_start();

    let mut total = 0;

    loop {
        let mut tasks = Vec::new();
        let mut complete = false;
        for _ in 0..jobs {
            let proxy = iterator.next_async().await;
            if proxy.is_none() {
                complete = true;
                break;
            }
            let proxy = proxy.unwrap();
            let task = collect_versions(db, proxy.to_string().to_lowercase());
            tasks.push(task);
        }
        let versions_vec = futures::future::join_all(tasks).await;

        let mut tasks = Vec::new();
        for versions in versions_vec.into_iter() {
            let versions = match versions {
                Ok(versions) => versions,
                Err(err) => {
                    error!(err = ?err, "Failed to collect versions");
                    continue;
                }
            };
            info!(count = versions.len(), "Collected versions");
            for version in versions.into_iter() {
                let task = entities::version::Entity::insert(version)
                    .on_conflict(OnConflict::new().do_nothing().to_owned())
                    .exec(db);
                tasks.push(task);
            }
        }
        total += tasks.len();
        futures::future::join_all(tasks).await;

        progress_span.pb_set_message(format!("Versioned {}", total).as_str());

        if complete {
            break;
        }
    }
    info!("Versioning finished");
}

/// Collect the implementation versions of a proxy.
/// The implementations are ordered by the block number (ascending).
async fn collect_versions(
    db: &DatabaseConnection,
    proxy: String,
) -> Result<Vec<entities::version::ActiveModel>, DbErr> {
    let window_size = 100;
    let mut offset = 0;
    let mut versions = Vec::new();
    loop {
        let versions_slice: Vec<(String, i64)> = entities::invocation::Entity::find()
            .select_only()
            .column(entities::invocation::Column::Implementation)
            .column_as(entities::invocation::Column::Block.min(), "min_block")
            .filter(entities::invocation::Column::Proxy.eq(proxy.clone()))
            .group_by(entities::invocation::Column::Implementation)
            .order_by_asc(entities::invocation::Column::Block.min())
            .offset(offset)
            .limit(window_size)
            .into_tuple()
            .all(db)
            .await?;
        let r_len = versions_slice.len();
        for v in versions_slice.into_iter() {
            let version = entities::version::ActiveModel {
                proxy: ActiveValue::Set(proxy.clone()),
                implementation: ActiveValue::Set(v.0),
                min_block: ActiveValue::Set(v.1),
                ..Default::default()
            };
            versions.push(version);
        }
        if r_len < window_size as usize {
            break;
        }
        offset += window_size;
    }
    Ok(versions)
}

#[cfg(test)]
mod tests {
    use libsofl_utils::config::Config;
    use proxyex_detector::config::ProxyExDetectorConfig;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_collect_single_version() {
        let cfg = ProxyExDetectorConfig::must_load();
        let db = cfg.db().await.unwrap();
        let versions = super::collect_versions(
            &db,
            "0x759b4da08fe959fde5bfc35bca733e79310c9531".to_string(),
        )
        .await
        .unwrap();
        assert_eq!(versions.len(), 1);
        let version = versions.get(0).unwrap();
        assert_eq!(
            version
                .implementation
                .clone()
                .into_value()
                .unwrap()
                .unwrap::<String>(),
            "0xf9e266af4bca5890e2781812cc6a6e89495a79f2"
        );
        assert_eq!(
            version
                .min_block
                .clone()
                .into_value()
                .unwrap()
                .unwrap::<i64>(),
            13065023i64
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_collect_multiple_versions() {
        let cfg = ProxyExDetectorConfig::must_load();
        let db = cfg.db().await.unwrap();
        let versions = super::collect_versions(
            &db,
            "0x05462671c05adc39a6521fa60d5e9443e9e9d2b9".to_string(),
        )
        .await
        .unwrap();
        assert_eq!(versions.len(), 9);
    }
}
