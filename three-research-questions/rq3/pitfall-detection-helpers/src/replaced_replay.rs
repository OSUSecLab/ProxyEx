use std::{collections::HashSet, sync::Arc};

use libsofl_core::{
    blockchain::{
        provider::{BcProvider, BcStateProvider},
        transaction::Tx,
    },
    conversion::ConvertTo,
    engine::{
        memory::MemoryBcState,
        state::BcState,
        transition::TransitionSpecBuilder,
        types::{Address, Bytecode, DatabaseRef, TxEnv, TxHash, U256},
    },
};

use crate::{entities, inspectors::collision::StorageAccessInspector};

#[derive(Debug)]
pub struct RegressionError {}

/// Replay a transaction and
/// simulate the transaction on alternative implementations at a specific block.
pub fn regression_one_tx<
    T: Tx,
    DB: DatabaseRef,
    P: BcProvider<T> + BcStateProvider<DB> + Sync + Send + 'static,
>(
    provider: Arc<P>,
    proxy: Address,
    implementation: Address,
    alt_implementations: Vec<(Address, Bytecode)>,
    tx: TxHash,
) -> Result<(StorageAccessInspector, Vec<StorageAccessInspector>), RegressionError>
where
    <DB as DatabaseRef>::Error: std::fmt::Debug,
{
    // replay the transaction
    let start_at = std::time::Instant::now();
    let tx = provider.tx(tx.cvt()).expect("tx not found");
    let mut tx_env = TxEnv::default();
    tx.fill_tx_env(&mut tx_env).unwrap();
    let tx_pos = tx.position().unwrap();
    let blk = tx_pos.block;
    let base_state = provider.bc_state_at(tx_pos).unwrap();
    let mut state = MemoryBcState::fork(&base_state);
    let spec = TransitionSpecBuilder::default()
        .at_block(provider.clone(), blk)
        .append_tx_env(tx_env.clone())
        .build();
    let mut replay_insp = StorageAccessInspector::new(proxy, implementation, 0, 0, true);
    state.transit(spec, &mut replay_insp).unwrap();
    replay_insp.time_elapsed = start_at.elapsed();

    let mut insps = Vec::new();
    // simulate the transaction on alternative implementations
    for (alt_impl, alt_code) in alt_implementations {
        let start_at = std::time::Instant::now();
        let mut state = MemoryBcState::fork(&base_state);
        state
            .replace_account_code(implementation, alt_code)
            .unwrap();
        let spec = TransitionSpecBuilder::default()
            .at_block(provider.clone(), blk)
            .bypass_check()
            .append_tx_env(tx_env.clone())
            .build();
        let mut insp =
            StorageAccessInspector::new_alt(proxy, implementation, alt_impl, 0, 0, false);
        state.transit(spec, &mut insp).unwrap();
        insp.time_elapsed = start_at.elapsed();
        insps.push(insp);
    }

    Ok((replay_insp, insps))
}

#[derive(Debug)]
pub struct RegressionIssue {
    pub proxy: Address,
    pub implementation: Address,
    pub alt_implementation: Address,
    pub tx: TxHash,
    pub original_sloads: HashSet<(U256, U256)>,
    pub original_sstores: HashSet<(U256, U256)>,
    pub alt_sloads: HashSet<(U256, U256)>,
    pub alt_sstores: HashSet<(U256, U256)>,
    pub different_slots: bool,
    pub different_values: bool,
    pub proxy_reverted: bool,
    pub time: i64, // macro seconds
}

impl From<RegressionIssue> for entities::regression::ActiveModel {
    fn from(issue: RegressionIssue) -> Self {
        Self {
            proxy: sea_orm::ActiveValue::Set(issue.proxy.to_string().to_lowercase()),
            implementation: sea_orm::ActiveValue::Set(
                issue.implementation.to_string().to_lowercase(),
            ),
            alt_implementation: sea_orm::ActiveValue::Set(
                issue.alt_implementation.to_string().to_lowercase(),
            ),
            tx: sea_orm::ActiveValue::Set(issue.tx.to_string()),
            original_sloads: sea_orm::ActiveValue::Set(
                serde_json::to_value(&issue.original_sloads).unwrap(),
            ),
            original_sstores: sea_orm::ActiveValue::Set(
                serde_json::to_value(&issue.original_sstores).unwrap(),
            ),
            alt_sloads: sea_orm::ActiveValue::Set(serde_json::to_value(&issue.alt_sloads).unwrap()),
            alt_sstores: sea_orm::ActiveValue::Set(
                serde_json::to_value(&issue.alt_sstores).unwrap(),
            ),
            different_slots: sea_orm::ActiveValue::Set(issue.different_slots),
            different_values: sea_orm::ActiveValue::Set(issue.different_values),
            proxy_reverted: sea_orm::ActiveValue::Set(issue.proxy_reverted),
            time: sea_orm::ActiveValue::Set(issue.time),
        }
    }
}

pub fn check_regression(
    original_insp: StorageAccessInspector,
    alt_insps: Vec<StorageAccessInspector>,
    tx: TxHash,
) -> Result<Vec<RegressionIssue>, RegressionError> {
    let mut rs = Vec::new();
    let mut original_sloads = HashSet::new();
    let mut original_sstores = HashSet::new();
    original_sloads.extend(original_insp.proxy_sloads);
    original_sstores.extend(original_insp.proxy_sstores);
    original_sloads.extend(original_insp.implementation_sloads);
    original_sstores.extend(original_insp.implementation_sstores);
    let mut original_access = HashSet::new();
    original_access.extend(&original_sloads);
    original_access.extend(&original_sstores);
    let original_slots: HashSet<U256> = original_access.iter().map(|(_, s, _)| *s).collect();
    let original_values: HashSet<(U256, U256)> =
        original_access.iter().map(|(_, s, v)| (*s, *v)).collect();

    for alt_insp in alt_insps {
        let mut alt_sloads = HashSet::new();
        let mut alt_sstores = HashSet::new();
        alt_sloads.extend(alt_insp.proxy_sloads);
        alt_sstores.extend(alt_insp.proxy_sstores);
        alt_sloads.extend(alt_insp.implementation_sloads);
        alt_sstores.extend(alt_insp.implementation_sstores);
        let alt_access = alt_sloads.union(&alt_sstores).collect::<Vec<_>>();
        let alt_slots: HashSet<U256> = alt_access.iter().map(|(_, s, _)| *s).collect();
        let alt_values: HashSet<(U256, U256)> =
            alt_access.iter().map(|(_, s, v)| (*s, *v)).collect();
        let different_slots = original_slots != alt_slots;
        let different_values = original_values != alt_values;
        rs.push(RegressionIssue {
            proxy: original_insp.proxy,
            implementation: original_insp.implementation,
            alt_implementation: alt_insp.alt_implementation.unwrap(),
            tx: tx,
            original_sloads: original_sloads.iter().map(|(_, s, v)| (*s, *v)).collect(),
            original_sstores: original_sstores.iter().map(|(_, s, v)| (*s, *v)).collect(),
            alt_sloads: alt_sloads.into_iter().map(|(_, s, v)| (s, v)).collect(),
            alt_sstores: alt_sstores.into_iter().map(|(_, s, v)| (s, v)).collect(),
            different_slots,
            different_values,
            proxy_reverted: alt_insp.proxy_reverted,
            time: alt_insp.time_elapsed.as_micros() as i64,
        });
    }
    Ok(rs)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use libsofl_core::{
        conversion::ConvertTo,
        engine::types::{Address, TxHash},
    };
    use libsofl_reth::{blockchain::provider::StateProviderFactory, config::RethConfig};
    use libsofl_utils::config::Config;

    use super::{check_regression, regression_one_tx};

    #[test]
    fn test_replaced_replay_audius_attack() {
        let attack_tx: TxHash =
            "0xfefd829e246002a8fd061eede7501bccb6e244a9aacea0ebceaecef5d877a984".cvt();
        let proxy: Address = "0x4DEcA517D6817B6510798b7328F2314d3003AbAC".cvt();
        let implementation: Address = "0x35dd16dfa4ea1522c29ddd087e8f076cad0ae5e8".cvt();
        let alt_impl: Address = "0x1c91af03a390b4c619b444425b3119e553b5b44b".cvt();
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let provider = Arc::new(provider);
        let alt_code = provider
            .bp
            .latest()
            .unwrap()
            .account_code(alt_impl)
            .unwrap()
            .unwrap()
            .bytes()
            .cvt();

        let (insp, insps) = regression_one_tx(
            provider.clone(),
            proxy,
            implementation,
            vec![(alt_impl, alt_code)],
            attack_tx,
        )
        .unwrap();
        let issues = check_regression(insp, insps, attack_tx).unwrap();
        assert!(issues
            .into_iter()
            .any(|i| (i.different_slots || i.different_values)));
    }

    #[test]
    fn test_replaced_comptroller_enter_markets() {
        let attack_tx: TxHash =
            "0xe8a31330950b545ce2bbc24c70882d736b6f070e27f1ca27c89b2dfd23327a08".cvt();
        let proxy: Address = "0xe2e17b2cbbf48211fa7eb8a875360e5e39ba2602".cvt();
        let implementation: Address = "0xd30378faec598befa2e419d2bcc0e17473965536".cvt();
        let alt_impl: Address = "0xaf082ef22e8c51357c10ffd157dc82f79ea09f39".cvt();
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let provider = Arc::new(provider);
        let alt_code = provider
            .bp
            .latest()
            .unwrap()
            .account_code(alt_impl)
            .unwrap()
            .unwrap()
            .bytes()
            .cvt();

        let (insp, insps) = regression_one_tx(
            provider.clone(),
            proxy,
            implementation,
            vec![(alt_impl, alt_code)],
            attack_tx,
        )
        .unwrap();
        let issues = check_regression(insp, insps, attack_tx).unwrap();
        assert!(issues
            .into_iter()
            .all(|i| !i.different_slots && !i.different_values));
    }

    #[test]
    fn test_replaced_root_chain_manager_exit() {
        let attack_tx: TxHash =
            "0x6303ba187ec21d1380ecfbd03f945bea0bb831f4212833920045ff893a3a9937".cvt();
        let proxy: Address = "0xa0c68c638235ee32657e8f720a23cec1bfc77c77".cvt();
        let implementation: Address = "0x0bff34272af650632236703a3d6d8e3c133421cb".cvt();
        let alt_impl: Address = "0x4015ccad9218b109d3339b356392c6ee8438e5d0".cvt();
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let provider = Arc::new(provider);
        let alt_code = provider
            .bp
            .latest()
            .unwrap()
            .account_code(alt_impl)
            .unwrap()
            .unwrap()
            .bytes()
            .cvt();

        let (insp, insps) = regression_one_tx(
            provider.clone(),
            proxy,
            implementation,
            vec![(alt_impl, alt_code)],
            attack_tx,
        )
        .unwrap();
        let issues = check_regression(insp, insps, attack_tx).unwrap();
        assert!(issues
            .into_iter()
            .all(|i| !i.different_slots && !i.different_values));
    }
}
