use libsofl_core::engine::{
    inspector::EvmInspector,
    state::BcState,
    types::{Address, Bytes, CallInputs, EVMData, Gas, Inspector, InstructionResult, U256},
};

pub struct Call {
    pub input: Bytes,
    pub value: U256,
}

pub struct CallExtractor {
    pub contract: Address,
    pub calls: Vec<Call>,
}

impl<S: BcState> Inspector<S> for CallExtractor {
    fn call(
        &mut self,
        _data: &mut EVMData<'_, S>,
        inputs: &mut CallInputs,
    ) -> (InstructionResult, Gas, Bytes) {
        if inputs.context.address == self.contract {
            self.calls.push(Call {
                input: inputs.input.clone(),
                value: inputs.transfer.value,
            });
        }
        (InstructionResult::Continue, Gas::new(0), Bytes::new())
    }
}

impl<S: BcState> EvmInspector<S> for CallExtractor {}

#[cfg(test)]
mod tests {
    use libsofl_core::{
        blockchain::{
            provider::{BcProvider, BcStateProvider},
            transaction::Tx,
        },
        conversion::ConvertTo,
        engine::{
            state::BcState,
            transition::TransitionSpecBuilder,
            types::{Address, Bytes, TxHash},
        },
    };
    use libsofl_reth::config::RethConfig;
    use libsofl_utils::config::Config;

    #[test]
    fn test_extract_initialize_call() {
        let p = RethConfig::must_load().bc_provider().unwrap();
        let tx: TxHash = "0xae8e542d4fdb5a6a33eeb129bb80f9bf23a1ceb3ef5f6caed1fd634ae3730c0b".cvt();
        let proxy: Address = "0x786dbff3f1292ae8f92ea68cf93c30b34b1ed04b".cvt();
        let tx = p.tx(tx.cvt()).unwrap();
        let pos = tx.position().unwrap();
        let mut state = p.bc_state_at(pos).unwrap();
        let mut extractor = super::CallExtractor {
            contract: proxy,
            calls: Vec::new(),
        };
        let spec = TransitionSpecBuilder::default()
            .at_block(&p, pos.block)
            .append_tx(tx)
            .build();
        state.transit(spec, &mut extractor).unwrap();
        let call = extractor.calls.get(0).unwrap();
        let input: Bytes = "0xd1f578940000000000000000000000003feab6f8510c73e05b8c0fdf96df012e3a14431900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000184c222ec8a00000000000000000000000087870bca3f3fd6335c3f4ce8392d69350b4fa4e200000000000000000000000040d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f0000000000000000000000008164cc65827dcfe994ab23944cbc90e0aa80bfcb000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000160000000000000000000000000000000000000000000000000000000000000001f4161766520457468657265756d205661726961626c6520446562742047484f0000000000000000000000000000000000000000000000000000000000000000127661726961626c654465627445746847484f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".cvt();
        assert_eq!(call.input, input);
    }
}
