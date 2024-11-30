use libsofl_core::engine::{
    inspector::EvmInspector,
    state::BcState,
    types::{
        Address, Bytes, CallInputs, CreateInputs, EVMData, Gas, Inspector, InstructionResult, 
    },
};
pub struct InitializeExtractor {
    pub contract: Address,
    pub created: bool,
    pub first_invoded: bool,
    pub caller_replaced: bool,
    pub initialize_input: Option<Bytes>,
}

impl InitializeExtractor {
    pub fn new(contract: Address) -> Self {
        Self {
            contract,
            created: false,
            first_invoded: false,
            caller_replaced: false,
            initialize_input: None,
        }
    }
}

impl<S: BcState> Inspector<S> for InitializeExtractor {
    fn create_end(
        &mut self,
        _data: &mut EVMData<'_, S>,
        _inputs: &CreateInputs,
        ret: InstructionResult,
        address: Option<Address>,
        remaining_gas: Gas,
        out: Bytes,
    ) -> (InstructionResult, Option<Address>, Gas, Bytes) {
        if let Some(addr) = address {
            if addr == self.contract {
                self.created = true;
            }
        }
        (ret, address, remaining_gas, out)
    }

    fn call(
        &mut self,
        _data: &mut EVMData<'_, S>,
        inputs: &mut CallInputs,
    ) -> (InstructionResult, Gas, Bytes) {
        if self.created && !self.first_invoded && inputs.context.code_address == self.contract {
            inputs.context.caller = Address::random();
            self.caller_replaced = true;
        }
        (InstructionResult::Continue, Gas::new(0), Bytes::new())
    }

    fn call_end(
        &mut self,
        _data: &mut EVMData<'_, S>,
        inputs: &CallInputs,
        remaining_gas: Gas,
        ret: InstructionResult,
        out: Bytes,
    ) -> (InstructionResult, Gas, Bytes) {
        if self.created
            && !self.first_invoded
            && self.caller_replaced
            && inputs.context.code_address == self.contract
        {
            if ret.is_ok() {
                // front-run success
                self.initialize_input = Some(inputs.input.clone());
            }
            self.first_invoded = true;
            self.caller_replaced = false;
        }
        (ret, remaining_gas, out)
    }
}

impl<S: BcState> EvmInspector<S> for InitializeExtractor {}

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
        let proxy: Address = "0x786dBff3f1292ae8F92ea68Cf93c30b34B1ed04B".cvt();
        let tx = p.tx(tx.cvt()).unwrap();
        let pos = tx.position().unwrap();
        let mut state = p.bc_state_at(pos).unwrap();
        let mut extractor = super::InitializeExtractor::new(proxy);
        let spec = TransitionSpecBuilder::default()
            .at_block(&p, pos.block)
            .append_tx(tx)
            .build();
        state.transit(spec, &mut extractor).unwrap();
        let initialize_input: Bytes = "0xd1f578940000000000000000000000003feab6f8510c73e05b8c0fdf96df012e3a14431900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000184c222ec8a00000000000000000000000087870bca3f3fd6335c3f4ce8392d69350b4fa4e200000000000000000000000040d16fc0246ad3160ccc09b8d0d3a2cd28ae6c2f0000000000000000000000008164cc65827dcfe994ab23944cbc90e0aa80bfcb000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000160000000000000000000000000000000000000000000000000000000000000001f4161766520457468657265756d205661726961626c6520446562742047484f0000000000000000000000000000000000000000000000000000000000000000127661726961626c654465627445746847484f0000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000".cvt();
        assert_eq!(extractor.initialize_input, Some(initialize_input));
    }
}
