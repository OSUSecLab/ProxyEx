use std::collections::HashSet;

use libsofl_core::engine::types::{Hash, U256};
use libsofl_utils::{
    config::Config,
    log::{config::LogConfig, error, info},
};
use proxyex_detector::{config::ProxyExDetectorConfig, entities};
use rayon::iter::{IntoParallelIterator, IntoParallelRefIterator, ParallelIterator};
use sea_orm::{
    sea_query::OnConflict, ColumnTrait, Condition, DbErr, EntityTrait, IntoActiveModel,
    PaginatorTrait, QueryFilter, QueryOrder,
};

#[tokio::main]
async fn main() -> Result<(), DbErr> {
    LogConfig::must_load().init();
    let cfg = ProxyExDetectorConfig::must_load();
    let db = cfg.db().await.unwrap();

    let mut paginator = entities::regression::Entity::find()
        .filter(
            Condition::all()
                .add(entities::regression::Column::DifferentSlots.eq(true))
                .add(entities::regression::Column::ProxyReverted.eq(false)),
        )
        .order_by_asc(entities::regression::Column::Proxy)
        .paginate(&db, 1000);
    let mut count = 0;
    while let Some(regressions) = paginator.fetch_and_next().await.unwrap() {
        info!("{} regressions fetched", regressions.len());
        let filtered_regressions = regressions
            .into_par_iter()
            .map(|r| {
                let (missed_slots, additional_slots) = check_one(&r);
                let missed_slots = serde_json::to_value(missed_slots).unwrap();
                let additional_slots = serde_json::to_value(additional_slots).unwrap();
                let model = entities::regression_filter::Model {
                    proxy: r.proxy,
                    tx: r.tx,
                    alt_implementation: r.alt_implementation,
                    implementation: r.implementation,
                    original_sloads: r.original_sloads,
                    original_sstores: r.original_sstores,
                    alt_sloads: r.alt_sloads,
                    alt_sstores: r.alt_sstores,
                    missed_slots,
                    additional_slots,
                };
                model.into_active_model()
            })
            .collect::<Vec<_>>();
        count += filtered_regressions.len();
        let r = entities::regression_filter::Entity::insert_many(filtered_regressions)
            .on_conflict(
                OnConflict::columns(vec![
                    entities::regression_filter::Column::Proxy,
                    entities::regression_filter::Column::Tx,
                    entities::regression_filter::Column::AltImplementation,
                ])
                .do_nothing()
                .to_owned(),
            )
            .exec(&db)
            .await;
        match r {
            Ok(_) => {}
            Err(e) => {
                if e != DbErr::RecordNotInserted {
                    error!("insert_many failed: {}", e);
                }
            }
        }
        info!("{} regressions filtered", count);
    }
    info!("finished");
    Ok(())
}

fn check_one(regression: &entities::regression::Model) -> (HashSet<U256>, HashSet<U256>) {
    let original_sloads: HashSet<(U256, U256)> =
        serde_json::from_value(regression.original_sloads.clone()).unwrap();
    let original_sstores: HashSet<(U256, U256)> =
        serde_json::from_value(regression.original_sstores.clone()).unwrap();
    let alt_sloads: HashSet<(U256, U256)> =
        serde_json::from_value(regression.alt_sloads.clone()).unwrap();
    let alt_sstores: HashSet<(U256, U256)> =
        serde_json::from_value(regression.alt_sstores.clone()).unwrap();
    let original_slots = original_sloads
        .iter()
        .map(|(slot, _)| slot)
        .chain(original_sstores.iter().map(|(slot, _)| slot))
        .collect::<HashSet<_>>();
    let alt_slots = alt_sloads
        .iter()
        .map(|(slot, _)| slot)
        .chain(alt_sstores.iter().map(|(slot, _)| slot))
        .collect::<HashSet<_>>();
    let missed_slots = original_slots
        .difference(&alt_slots)
        .cloned()
        .cloned()
        .collect::<HashSet<_>>();
    let additional_slots = alt_slots
        .difference(&original_slots)
        .cloned()
        .cloned()
        .collect::<HashSet<_>>();
    (missed_slots, additional_slots)
}
