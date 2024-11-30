use std::{collections::HashSet, io::Write};

use clap::{command, Parser};
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
};
use proxyex_detector::{
    config::ProxyExDetectorConfig, entities, inspectors::collision::StorageAccessInspector,
};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use sea_orm::{ColumnTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'l', long, default_value = "info")]
    log_level: String,

    #[arg(short, long, default_value = "1")]
    jobs: usize,

    #[arg(short, long)]
    output: String,

    proxies: String,
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> Result<(), DbErr> {
    let args = Cli::parse();
    LogConfig::must_load().init();

    let provider = RethConfig::must_load().bc_provider().unwrap();
    let db = ProxyExDetectorConfig::must_load().db().await?;

    let proxies = args
        .proxies
        .split(',')
        .map(|s| ConvertTo::<Address>::cvt(&s))
        .collect::<Vec<_>>();

    let infos = analyze_all(&provider, &db, proxies.clone()).await.unwrap();

    let mut output_file = std::fs::File::create(args.output).unwrap();
    for (proxy, info) in proxies.into_iter().zip(infos.into_iter()) {
        let line = format!(
            "{}\t{}\t{}\n",
            proxy.to_string().to_lowercase(),
            serde_json::to_string(&info.conflict_slots)
                .unwrap()
                .to_string(),
            serde_json::to_string(&info.conflict_points)
                .unwrap()
                .to_string()
        );
        output_file.write(line.as_bytes()).unwrap();
        output_file.flush().unwrap();
    }

    Ok(())
}

#[derive(Debug)]
struct Info {
    pub conflict_slots: HashSet<U256>,
    pub conflict_points: Vec<(TxHash, Vec<(U256, U256, String)>)>,
}

async fn analyze_all(
    provider: &RethProvider,
    db: &DatabaseConnection,
    proxies: Vec<Address>,
) -> Result<Vec<Info>, DbErr> {
    let mut infos = Vec::new();
    for proxy in proxies {
        let info = analyze_one(provider, db, proxy).await?;
        infos.push(info);
    }
    Ok(infos)
}

async fn analyze_one(
    provider: &RethProvider,
    db: &DatabaseConnection,
    proxy: Address,
) -> Result<Info, DbErr> {
    let invocations = entities::invocation::Entity::find()
        .filter(entities::invocation::Column::Proxy.eq(proxy.to_string().to_lowercase()))
        .all(db)
        .await?;
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
        invocations = invocations.len(),
        "analyzing"
    );

    let points: Vec<(TxHash, Vec<(U256, U256, String)>)> = invocations
        .par_iter()
        .map(|inv| {
            let tx_hash: TxHash = inv.tx.cvt();
            let tx = provider.tx(tx_hash.cvt()).unwrap();
            let pos = tx.position().unwrap();
            let mut state = provider.bc_state_at(pos).unwrap();
            let spec = TransitionSpecBuilder::default()
                .at_block(provider, pos.block)
                .append_tx(tx)
                .build();
            let mut insp =
                StorageAccessInspector::new(proxy, inv.implementation.cvt(), 0, 1, false);
            state.transit(spec, &mut insp).unwrap();
            let mut sstores = Vec::new();
            for proxy_sstore in insp.proxy_sstores {
                let t = (proxy_sstore.0.to_string().to_lowercase(), proxy_sstore.1);
                if conflicts.contains(&t) {
                    sstores.push((proxy_sstore.1, proxy_sstore.2, "proxy".to_string()));
                }
            }
            for impl_sstore in insp.implementation_sstores {
                let t = (impl_sstore.0.to_string().to_lowercase(), impl_sstore.1);
                if conflicts.contains(&t) {
                    sstores.push((impl_sstore.1, impl_sstore.2, inv.implementation.clone()));
                }
            }
            (tx_hash, sstores)
        })
        .collect();

    Ok(Info {
        conflict_slots: conflicts.iter().map(|(_, s)| s.clone()).collect(),
        conflict_points: points,
    })
}
