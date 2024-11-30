use libsofl_core::engine::{
    inspector::EvmInspector,
    state::BcState,
    types::{
        opcode, Address, Bytes, CallInputs, CallScheme, EVMData, Gas, Inspector, InstructionResult,
        Interpreter,
    },
};

pub struct HasDelegateCallOrNot {
    pub contract: Address,
    pub has_delegatecall: bool,
    pub updated_contract: bool,
}

impl<S: BcState> Inspector<S> for HasDelegateCallOrNot {
    fn step(&mut self, interp: &mut Interpreter<'_>, _data: &mut EVMData<'_, S>) {
        let opcode = interp.current_opcode();
        if interp.contract().address == self.contract && opcode == opcode::SSTORE {
            self.updated_contract = true;
        }
    }

    fn call(
        &mut self,
        _data: &mut EVMData<'_, S>,
        inputs: &mut CallInputs,
    ) -> (InstructionResult, Gas, Bytes) {
        if inputs.context.scheme == CallScheme::DelegateCall {
            self.has_delegatecall = true;
        }
        (InstructionResult::Continue, Gas::new(0), Bytes::new())
    }
}

impl<S: BcState> EvmInspector<S> for HasDelegateCallOrNot {}
