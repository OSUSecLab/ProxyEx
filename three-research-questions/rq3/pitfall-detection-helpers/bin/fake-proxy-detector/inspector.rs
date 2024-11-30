use libsofl_core::engine::{
    inspector::EvmInspector,
    state::BcState,
    types::{Address, Bytes, CallInputs, CallScheme, EVMData, Gas, Inspector, InstructionResult},
};

pub struct ImplInspector {
    pub proxy: Address,
    pub implementation: Option<Address>,
}

impl<S: BcState> Inspector<S> for ImplInspector {
    fn call(
        &mut self,
        data: &mut EVMData<'_, S>,
        inputs: &mut CallInputs,
    ) -> (InstructionResult, Gas, Bytes) {
        if data.journaled_state.depth() == 1
            && inputs.context.scheme == CallScheme::DelegateCall
            && inputs.context.address == self.proxy
        {
            self.implementation = Some(inputs.context.code_address);
        }
        (InstructionResult::Continue, Gas::new(0), Bytes::new())
    }
}

impl<S: BcState> EvmInspector<S> for ImplInspector {}
