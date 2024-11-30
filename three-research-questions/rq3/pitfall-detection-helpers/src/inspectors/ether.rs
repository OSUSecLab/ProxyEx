use libsofl_core::engine::{
    inspector::EvmInspector,
    state::BcState,
    types::{Bytes, CallInputs, EVMData, Gas, Inspector, InstructionResult},
};

pub struct EtherInspector {}

impl<S: BcState> Inspector<S> for EtherInspector {
    fn call(
        &mut self,
        _data: &mut EVMData<'_, S>,
        inputs: &mut CallInputs,
    ) -> (InstructionResult, Gas, Bytes) {
        println!(
            "call: from={}, to={}, value={}",
            inputs.transfer.source, inputs.transfer.target, inputs.transfer.value
        );
        (InstructionResult::Continue, Gas::new(0), Bytes::new())
    }
}

impl<S: BcState> EvmInspector<S> for EtherInspector {}

#[cfg(test)]
mod tests {
    use libsofl_core::{
        blockchain::{
            provider::{BcProvider, BcStateProvider},
            transaction::Tx,
        },
        conversion::ConvertTo,
        engine::{state::BcState, transition::TransitionSpecBuilder, types::TxHash},
    };
    use libsofl_reth::config::RethConfig;
    use libsofl_utils::config::Config;

    #[test]
    fn test_one_tx() {
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let tx_hash: TxHash =
            "0x0cd617a3cc204b159c2a88cf64b559a46d8b9c93cd3c2d7abe7adcbb132a73d8".cvt();
        let tx = provider.tx(tx_hash.cvt()).unwrap();
        let mut state = provider.bc_state_at(tx.position().unwrap()).unwrap();
        let mut inspector = super::EtherInspector {};
        let spec = TransitionSpecBuilder::default()
            .at_block(&provider, tx.position().unwrap().block)
            .append_tx(tx)
            .build();
        state.transit(spec, &mut inspector).unwrap();
    }
}
