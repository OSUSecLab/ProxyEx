use std::{collections::HashSet, sync::Arc, time::Duration};

use crossbeam::channel::{self, Sender};
use indicatif::ProgressStyle;
use libsofl_core::{
    blockchain::{
        provider::{BcProvider, BcStateProvider},
        transaction::Tx,
    },
    conversion::ConvertTo,
    engine::{
        state::BcState,
        transition::TransitionSpecBuilder,
        types::{Address, DatabaseRef, TxHash, U256},
    },
};
use libsofl_reth::blockchain::provider::RethProvider;
use libsofl_utils::log::{debug, error, info, info_span};
use sea_orm::ActiveValue;
use tracing_indicatif::span_ext::IndicatifSpanExt;

use crate::{entities, inspectors::collision::StorageAccessInspector, pool::FIFOTaskPool};

#[derive(Debug, serde::Serialize)]
pub struct ReplayError {
    pub proxy: Address,
    pub tx: TxHash,
    pub index: usize,
    pub total: usize,
    pub msg: String,
}

impl ReplayError {
    pub fn new(proxy: Address, tx: TxHash, index: usize, total: usize, msg: String) -> Self {
        Self {
            total,
            tx,
            proxy,
            index,
            msg,
        }
    }
}

pub struct OriginalReplayScheduler {
    p: Arc<RethProvider>,
    pool: FIFOTaskPool<Result<(TxHash, StorageAccessInspector), ReplayError>>,
    result_thread: std::thread::JoinHandle<()>,
}

impl OriginalReplayScheduler {
    pub fn new(
        p: Arc<RethProvider>,
        n_threads: usize,
        result_tx: Sender<Result<SlotCollisionResult, ReplayError>>,
    ) -> Self {
        let (tx_result_tx, tx_result_rv) = channel::bounded(n_threads * 2);
        let pool = FIFOTaskPool::new(tx_result_tx, n_threads);

        // result thread aggregate tx replay results into proxy results
        let result_thread = std::thread::spawn(move || {
            info!("tx replay aggregator thread started");

            let mut proxy_span = info_span!("proxy");

            let mut insps = Vec::new();
            loop {
                debug!("waiting for one tx replay result");
                let r: Result<(TxHash, StorageAccessInspector), ReplayError> =
                    match tx_result_rv.recv() {
                        Ok(r) => {
                            debug!("got one tx replay result");
                            r
                        }
                        Err(_) => {
                            debug!("result channel closed, result aggregator thread exits");
                            break;
                        }
                    };
                let (tx, proxy, index, total) = match &r {
                    Ok(r) => (r.0, r.1.proxy, r.1.index, r.1.total),
                    Err(e) => (e.tx, e.proxy, e.index, e.total),
                };

                if insps.len() == 0 {
                    drop(proxy_span);
                    // create a new progress bar for a new proxy
                    info!(
                        proxy = proxy.to_string().to_lowercase().as_str(),
                        "Replaying"
                    );
                    proxy_span = info_span!("proxy");
                    proxy_span.pb_set_style(&ProgressStyle::default_bar());
                    proxy_span.pb_set_length(total as u64);
                    proxy_span.pb_set_message(
                        format!("proxy: {}", proxy.to_string().to_lowercase()).as_str(),
                    );
                    proxy_span.pb_start();
                }
                proxy_span.pb_set_position(index as u64 + 1);

                insps.push(r);
                if index + 1 == total {
                    debug!("one proxy analysis finished, yielding");
                    // one proxy finishes
                    assert_eq!(insps.len(), total);
                    let is: Result<Vec<(TxHash, StorageAccessInspector)>, ReplayError> =
                        insps.into_iter().collect();
                    match is {
                        // Ok(is) => {
                        //     let is = is.into_iter().map(|x| x.1).collect();
                        //     let result = OriginalReplayResult::new(&is);
                        //     let _ = result_tx.send(Ok(result));
                        // }
                        Ok(is) => {
                            let result = SlotCollisionResult::new(&is);
                            let _ = result_tx.send(Ok(result));
                        }
                        Err(e) => {
                            let _ = result_tx.send(Err(e));
                        }
                    };
                    insps = Vec::new();
                } else {
                    debug!(
                        index = index,
                        total = total,
                        "one tx replay finished, waiting for more"
                    )
                }
            }
        });

        Self {
            p,
            pool,
            result_thread,
        }
    }

    pub fn feed_proxy_invocation_in_order(
        &self,
        proxy: Address,
        implementation: Address,
        tx: TxHash,
        index: usize,
        total: usize,
    ) {
        let p = self.p.clone();
        self.pool
            .add_task(move || replay_one_tx(p, proxy, implementation, tx, index, total))
    }

    pub fn close(self) {
        self.pool.close();
        self.result_thread
            .join()
            .expect("result thread join failed");
    }
}

pub fn replay_one_tx<
    T: Tx,
    DB: DatabaseRef,
    P: BcProvider<T> + BcStateProvider<DB> + Sync + Send + 'static,
>(
    provider: Arc<P>,
    proxy: Address,
    implementation: Address,
    tx: TxHash,
    index: usize,
    total: usize,
) -> Result<(TxHash, StorageAccessInspector), ReplayError>
where
    <DB as DatabaseRef>::Error: std::fmt::Debug,
{
    let tx_hash = tx;
    debug!(
        proxy = proxy.to_string().to_lowercase().as_str(),
        tx = tx.to_string().to_lowercase().as_str(),
        "Replaying"
    );

    let mut insp = StorageAccessInspector::new(proxy, implementation, index, total, false);

    let start_at = std::time::Instant::now();

    let tx = provider.tx(tx.cvt()).map_err(|e| {
        let msg = format!("Error: {:?}", e);
        error!("error {:?}", e);
        ReplayError::new(proxy, tx_hash, index, total, msg)
    })?;
    let tx_pos = tx.position().unwrap();
    let mut state = provider.bc_state_at(tx_pos).map_err(|e| {
        let msg = format!("Error: {:?}", e);
        ReplayError::new(proxy, tx_hash, index, total, msg)
    })?;
    let spec = TransitionSpecBuilder::new()
        .at_block(&provider, tx_pos.block)
        .append_tx(tx)
        .build();

    let _ = state.transit(spec, &mut insp).map_err(|e| {
        let msg = format!("Error: {:?}", e);
        ReplayError::new(proxy, tx_hash, index, total, msg)
    })?;

    insp.time_elapsed = start_at.elapsed();

    Ok((tx_hash, insp))
}

#[derive(Debug, serde::Serialize)]
pub struct OriginalReplayResult {
    pub proxy: Address,
    pub proxy_sstores: HashSet<(Address, U256)>,
    pub proxy_sloads: HashSet<(Address, U256)>,
    pub implementation_ssotres: HashSet<(Address, U256)>,
    pub implementation_sloads: HashSet<(Address, U256)>,
    pub problematic: bool,
    pub total_time: Duration,
    pub avg_time: Duration,
}

impl From<OriginalReplayResult> for entities::replay::ActiveModel {
    fn from(r: OriginalReplayResult) -> Self {
        Self {
            proxy: ActiveValue::Set(r.proxy.to_string().to_lowercase()),
            problematic: ActiveValue::Set(r.problematic),
            proxy_sstores: ActiveValue::Set(serde_json::to_value(r.proxy_sstores).unwrap()),
            proxy_sloads: ActiveValue::Set(serde_json::to_value(r.proxy_sloads).unwrap()),
            implementation_sstores: ActiveValue::Set(
                serde_json::to_value(r.implementation_ssotres).unwrap(),
            ),
            implementation_sloads: ActiveValue::Set(
                serde_json::to_value(r.implementation_sloads).unwrap(),
            ),
            total_time: ActiveValue::Set(r.total_time.as_millis() as i64),
            avg_time: ActiveValue::Set(r.avg_time.as_millis() as i64),
        }
    }
}

impl OriginalReplayResult {
    pub fn new(insps: &Vec<StorageAccessInspector>) -> Self {
        let mut proxy_sstores = HashSet::new();
        let mut proxy_sloads = HashSet::new();
        let mut implementation_ssotres = HashSet::new();
        let mut implementation_sloads = HashSet::new();
        let proxy = insps[0].proxy;
        for insp in insps {
            assert_eq!(proxy, insp.proxy);
            proxy_sstores.extend(
                &insp
                    .proxy_sstores
                    .iter()
                    .map(|(a, s, _)| (*a, *s))
                    .collect::<HashSet<(Address, U256)>>(),
            );
            proxy_sloads.extend(
                &insp
                    .proxy_sloads
                    .iter()
                    .map(|(a, s, _)| (*a, *s))
                    .collect::<HashSet<(Address, U256)>>(),
            );
            implementation_ssotres.extend(
                &insp
                    .implementation_sstores
                    .iter()
                    .map(|(a, s, _)| (*a, *s))
                    .collect::<HashSet<(Address, U256)>>(),
            );
            implementation_sloads.extend(
                &insp
                    .implementation_sloads
                    .iter()
                    .map(|(a, s, _)| (*a, *s))
                    .collect::<HashSet<(Address, U256)>>(),
            );
        }
        let problematic = proxy_sstores.intersection(&implementation_ssotres).count() > 0;
        let total_time: Duration = insps.iter().map(|i| i.time_elapsed).sum();
        let avg_time = total_time.div_f32(insps.len() as f32);
        Self {
            proxy,
            proxy_sstores,
            proxy_sloads,
            implementation_ssotres,
            implementation_sloads,
            problematic,
            total_time,
            avg_time,
        }
    }
}

#[derive(Debug, serde::Serialize)]
pub struct SlotCollisionResult {
    pub proxy: Address,
    pub proxy_sstores: Vec<(TxHash, HashSet<(U256, U256)>)>,
    pub proxy_sloads: Vec<(TxHash, HashSet<(U256, U256)>)>,
    pub implementation_ssotres: Vec<(TxHash, HashSet<(U256, U256)>)>,
    pub implementation_sloads: Vec<(TxHash, HashSet<(U256, U256)>)>,
    pub problematic: bool,
    pub total_time: Duration,
    pub avg_time: Duration,
}

impl From<SlotCollisionResult> for entities::collision::ActiveModel {
    fn from(r: SlotCollisionResult) -> Self {
        Self {
            proxy: ActiveValue::Set(r.proxy.to_string().to_lowercase()),
            problematic: ActiveValue::Set(r.problematic),
            proxy_sstores: ActiveValue::Set(serde_json::to_value(r.proxy_sstores).unwrap()),
            proxy_sloads: ActiveValue::Set(serde_json::to_value(r.proxy_sloads).unwrap()),
            implementation_sstores: ActiveValue::Set(
                serde_json::to_value(r.implementation_ssotres).unwrap(),
            ),
            implementation_sloads: ActiveValue::Set(
                serde_json::to_value(r.implementation_sloads).unwrap(),
            ),
            total_time: ActiveValue::Set(r.total_time.as_millis() as i64),
            avg_time: ActiveValue::Set(r.avg_time.as_millis() as i64),
        }
    }
}

impl SlotCollisionResult {
    pub fn new(insps: &Vec<(TxHash, StorageAccessInspector)>) -> Self {
        let mut proxy_sstores = Vec::new();
        let mut proxy_sloads = Vec::new();
        let mut implementation_ssotres = Vec::new();
        let mut implementation_sloads = Vec::new();
        let proxy = insps[0].1.proxy;
        for (tx, insp) in insps {
            assert_eq!(proxy, insp.proxy);
            proxy_sstores.push((
                *tx,
                insp.proxy_sstores
                    .iter()
                    .map(|(_, slot, value)| (*slot, *value))
                    .collect::<HashSet<_>>(),
            ));
            proxy_sloads.push((
                *tx,
                insp.proxy_sloads
                    .iter()
                    .map(|(_, slot, value)| (*slot, *value))
                    .collect::<HashSet<_>>(),
            ));
            implementation_ssotres.push((
                *tx,
                insp.implementation_sstores
                    .iter()
                    .map(|(_, slot, value)| (*slot, *value))
                    .collect::<HashSet<_>>(),
            ));
            implementation_sloads.push((
                *tx,
                insp.implementation_sloads
                    .iter()
                    .map(|(_, slot, value)| (*slot, *value))
                    .collect::<HashSet<_>>(),
            ));
        }
        // Both proxy and impl write to the same slot and this slot is read by at least one of them.
        let proxy_write_slots = proxy_sstores
            .iter()
            .map(|(_, s)| s.iter().map(|(s, _)| *s).collect::<HashSet<_>>())
            .fold(HashSet::new(), |acc, s| acc.union(&s).cloned().collect());
        let proxy_read_slots = proxy_sloads
            .iter()
            .map(|(_, s)| s.iter().map(|(s, _)| *s).collect::<HashSet<_>>())
            .fold(HashSet::new(), |acc, s| acc.union(&s).cloned().collect());
        let impl_write_slots = implementation_ssotres
            .iter()
            .map(|(_, s)| s.iter().map(|(s, _)| *s).collect::<HashSet<_>>())
            .fold(HashSet::new(), |acc, s| acc.union(&s).cloned().collect());
        let impl_read_slots = implementation_sloads
            .iter()
            .map(|(_, s)| s.iter().map(|(s, _)| *s).collect::<HashSet<_>>())
            .fold(HashSet::new(), |acc, s| acc.union(&s).cloned().collect());
        let write_write_slots = proxy_write_slots
            .intersection(&impl_write_slots)
            .cloned()
            .collect::<HashSet<_>>();
        let read_slots = proxy_read_slots
            .union(&impl_read_slots)
            .cloned()
            .collect::<HashSet<_>>();
        let read_write_slots = read_slots
            .intersection(&write_write_slots)
            .cloned()
            .collect::<HashSet<_>>();
        let problematic = read_write_slots.len() > 0;

        // filter out slots
        let proxy_sstores = proxy_sstores
            .into_iter()
            .map(|(tx, s)| {
                (
                    tx,
                    s.into_iter()
                        .filter(|(s, _)| read_write_slots.contains(s))
                        .collect::<HashSet<_>>(),
                )
            })
            .filter(|(_, s)| s.len() > 0)
            .collect::<Vec<_>>();
        let proxy_sloads = proxy_sloads
            .into_iter()
            .map(|(tx, s)| {
                (
                    tx,
                    s.into_iter()
                        .filter(|(s, _)| read_write_slots.contains(s))
                        .collect::<HashSet<_>>(),
                )
            })
            .filter(|(_, s)| s.len() > 0)
            .collect::<Vec<_>>();
        let implementation_ssotres = implementation_ssotres
            .into_iter()
            .map(|(tx, s)| {
                (
                    tx,
                    s.into_iter()
                        .filter(|(s, _)| read_write_slots.contains(s))
                        .collect::<HashSet<_>>(),
                )
            })
            .filter(|(_, s)| s.len() > 0)
            .collect::<Vec<_>>();
        let implementation_sloads = implementation_sloads
            .into_iter()
            .map(|(tx, s)| {
                (
                    tx,
                    s.into_iter()
                        .filter(|(s, _)| read_write_slots.contains(s))
                        .collect::<HashSet<_>>(),
                )
            })
            .filter(|(_, s)| s.len() > 0)
            .collect::<Vec<_>>();

        let total_time: Duration = insps.iter().map(|i| i.1.time_elapsed).sum();
        let avg_time = total_time.div_f32(insps.len() as f32);
        Self {
            proxy,
            proxy_sstores,
            proxy_sloads,
            implementation_ssotres,
            implementation_sloads,
            problematic: false,
            total_time,
            avg_time,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use libsofl_core::conversion::ConvertTo;
    use libsofl_reth::config::RethConfig;
    use libsofl_utils::config::Config;

    use crate::dataset::ProxyData;

    #[test]
    fn test_replay_one_proxy() {
        let data = r#"{"proxy": "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84", "impls": [{"tx": "0x5ae770f17756d7484c18a7db38adc68ce12ee5ad6189ee1f444c1a719062febd", "impl": "0x17144556fd3424edc8fc8a4c940b2d04936d17eb", "block": 
        18935916}]}"#;
        let d: ProxyData = serde_json::from_str(data).unwrap();
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let provider = Arc::new(provider);
        let (result_tx, result_rv) = crossbeam::channel::bounded(1);
        let scheduler = super::OriginalReplayScheduler::new(provider, 1, result_tx);
        scheduler.feed_proxy_invocation_in_order(
            d.proxy.cvt(),
            d.impls[0].implementation.cvt(),
            d.impls[0].tx.cvt(),
            0,
            1,
        );
        let r = result_rv.recv().unwrap().unwrap();
        assert_eq!(r.implementation_ssotres.len(), 1);
    }

    #[test]
    fn test_replay_failure() {
        let data = r#"{"proxy": "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84", "impls": [{"tx": "0x5ae770f17756d7484c18a7db38adc68ce12ee5ad6189ee1f444c1a719062febe", "impl": "0x17144556fd3424edc8fc8a4c940b2d04936d17eb", "block": 
        18935916}]}"#; // tx_hash is not correct, replay should fail
        let d: ProxyData = serde_json::from_str(data).unwrap();
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let provider = Arc::new(provider);
        let (result_tx, result_rv) = crossbeam::channel::bounded(1);
        let scheduler = super::OriginalReplayScheduler::new(provider, 1, result_tx);
        scheduler.feed_proxy_invocation_in_order(
            d.proxy.cvt(),
            d.impls[0].implementation.cvt(),
            d.impls[0].tx.cvt(),
            0,
            1,
        );
        let e = result_rv.recv().unwrap().unwrap_err();
        assert_eq!(
            ConvertTo::<String>::cvt(&e.proxy),
            "0xae7ab96520DE3A18E5e111B5EaAb095312D7fE84"
        );
    }
}
