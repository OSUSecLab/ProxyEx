use core::panic;
use std::{collections::HashSet, time::Duration};

use libsofl_core::engine::{
    inspector::EvmInspector,
    state::BcState,
    types::{
        opcode, Address, Bytes, CallInputs, CreateInputs, EVMData, Gas, Inspector,
        InstructionResult, Interpreter, U256,
    },
};

/// StorageCollisionInspector checks whether the transaction to a proxy contract has the following scenario:
/// 1. Proxy contract write the same storage slot as the implementation contract.
#[derive(Debug)]
pub struct StorageAccessInspector {
    // input
    pub index: usize,
    pub total: usize,
    pub proxy: Address,
    pub implementation: Address,
    pub alt_implementation: Option<Address>,
    pub ignore_failed_calls: bool,

    // output
    pub proxy_reverted: bool,
    pub proxy_created: bool,
    pub time_elapsed: Duration,
    pub proxy_sstores: HashSet<(Address, U256, U256)>,
    pub proxy_sloads: HashSet<(Address, U256, U256)>,
    pub implementation_sstores: HashSet<(Address, U256, U256)>,
    pub implementation_sloads: HashSet<(Address, U256, U256)>,

    // call stack
    pub _proxy_sstores: Vec<HashSet<(Address, U256, U256)>>,
    pub _proxy_sloads: Vec<HashSet<(Address, U256, U256)>>,
    pub _implementation_sstores: Vec<HashSet<(Address, U256, U256)>>,
    pub _implementation_sloads: Vec<HashSet<(Address, U256, U256)>>,

    // internal
    code_address: Vec<Option<Address>>,
    state_address: Vec<Option<Address>>,
}

impl StorageAccessInspector {
    pub fn new(
        proxy: Address,
        implementation: Address,
        index: usize,
        total: usize,
        ignore_failed_calls: bool,
    ) -> Self {
        Self {
            time_elapsed: Duration::new(0, 0),
            total,
            proxy,
            index,
            proxy_sstores: HashSet::new(),
            proxy_sloads: HashSet::new(),
            implementation,
            alt_implementation: None,
            implementation_sstores: HashSet::new(),
            implementation_sloads: HashSet::new(),
            code_address: Vec::new(),
            state_address: Vec::new(),
            ignore_failed_calls,
            proxy_reverted: false,
            proxy_created: false,

            _proxy_sstores: Vec::new(),
            _proxy_sloads: Vec::new(),
            _implementation_sstores: Vec::new(),
            _implementation_sloads: Vec::new(),
        }
    }

    pub fn new_alt(
        proxy: Address,
        implementation: Address,
        alt_implementation: Address,
        index: usize,
        total: usize,
        ignore_failed_calls: bool,
    ) -> Self {
        Self {
            time_elapsed: Duration::new(0, 0),
            total,
            proxy,
            index,
            proxy_sstores: HashSet::new(),
            proxy_sloads: HashSet::new(),
            implementation,
            alt_implementation: Some(alt_implementation),
            implementation_sstores: HashSet::new(),
            implementation_sloads: HashSet::new(),
            code_address: Vec::new(),
            state_address: Vec::new(),
            ignore_failed_calls,
            proxy_reverted: false,
            proxy_created: false,

            _proxy_sstores: Vec::new(),
            _proxy_sloads: Vec::new(),
            _implementation_sstores: Vec::new(),
            _implementation_sloads: Vec::new(),
        }
    }

    pub fn set_implementation(&mut self, implementation: Address) {
        self.implementation = implementation;
    }
}

impl<S: BcState> Inspector<S> for StorageAccessInspector {
    #[inline]
    fn step(&mut self, interp: &mut Interpreter<'_>, data: &mut EVMData<'_, S>) {
        if self.code_address.is_empty() {
            panic!("code_address is empty: {:?}", self.proxy);
        }
        let current_code_addr: Address = self
            .code_address
            .last()
            .unwrap()
            .unwrap_or(interp.contract().address); // None when current call is create
        let current_state_addr = self
            .state_address
            .last()
            .unwrap()
            .unwrap_or(interp.contract().address); // None when current call is create
        if current_code_addr != self.proxy && current_code_addr != self.implementation
            || current_state_addr != self.proxy
        {
            return;
        }

        let current_state_addr = self
            .state_address
            .last()
            .unwrap()
            .unwrap_or(interp.contract().address);

        let op = interp.current_opcode();
        match op {
            opcode::SSTORE | opcode::TSTORE => {
                let key = interp.stack().peek(0).unwrap();
                let value = interp.stack().peek(1).unwrap();
                if current_code_addr == self.proxy {
                    self._proxy_sstores.last_mut().unwrap().insert((
                        current_state_addr,
                        key,
                        value,
                    ));
                } else {
                    self._implementation_sstores.last_mut().unwrap().insert((
                        current_state_addr,
                        key,
                        value,
                    ));
                }
            }
            opcode::SLOAD | opcode::TLOAD => {
                let key = interp.stack().peek(0).unwrap();
                let value = data.db.storage(current_state_addr, key).unwrap();
                if current_code_addr == self.proxy {
                    self._proxy_sloads
                        .last_mut()
                        .unwrap()
                        .insert((current_state_addr, key, value));
                } else {
                    self._implementation_sloads.last_mut().unwrap().insert((
                        current_state_addr,
                        key,
                        value,
                    ));
                }
            }
            _ => {}
        }
    }
    #[inline]
    fn call(
        &mut self,
        _data: &mut EVMData<'_, S>,
        inputs: &mut CallInputs,
    ) -> (InstructionResult, Gas, Bytes) {
        let code_addr = inputs.context.code_address;
        let state_addr = inputs.context.address;
        self.code_address.push(Some(code_addr));
        self.state_address.push(Some(state_addr));
        self._proxy_sloads.push(HashSet::new());
        self._proxy_sstores.push(HashSet::new());
        self._implementation_sloads.push(HashSet::new());
        self._implementation_sstores.push(HashSet::new());
        (InstructionResult::Continue, Gas::new(0), Bytes::new())
    }

    #[inline]
    fn call_end(
        &mut self,
        _data: &mut EVMData<'_, S>,
        _inputs: &CallInputs,
        remaining_gas: Gas,
        ret: InstructionResult,
        out: Bytes,
    ) -> (InstructionResult, Gas, Bytes) {
        let current_code_addr = self.code_address.pop().unwrap().unwrap();
        let _ = self.state_address.pop().unwrap().unwrap();
        let proxy_sloads = self._proxy_sloads.pop().unwrap();
        let proxy_sstores = self._proxy_sstores.pop().unwrap();
        let implementation_sloads = self._implementation_sloads.pop().unwrap();
        let implementation_sstores = self._implementation_sstores.pop().unwrap();
        if !ret.is_ok() && current_code_addr == self.proxy {
            self.proxy_reverted = true;
        }
        self._proxy_sloads.last_mut().unwrap().extend(proxy_sloads);
        self._proxy_sstores
            .last_mut()
            .unwrap()
            .extend(proxy_sstores);
        self._implementation_sloads
            .last_mut()
            .unwrap()
            .extend(implementation_sloads);
        self._implementation_sstores
            .last_mut()
            .unwrap()
            .extend(implementation_sstores);
        (ret, remaining_gas, out)
    }

    #[inline]
    fn create(
        &mut self,
        _data: &mut EVMData<'_, S>,
        _inputs: &mut CreateInputs,
    ) -> (InstructionResult, Option<Address>, Gas, Bytes) {
        self.code_address.push(None);
        self.state_address.push(None);
        self._proxy_sloads.push(HashSet::new());
        self._proxy_sstores.push(HashSet::new());
        self._implementation_sloads.push(HashSet::new());
        self._implementation_sstores.push(HashSet::new());
        (InstructionResult::Continue, None, Gas::new(0), Bytes::new())
    }

    #[inline]
    fn create_end(
        &mut self,
        _data: &mut EVMData<'_, S>,
        _inputs: &CreateInputs,
        ret: InstructionResult,
        address: Option<Address>,
        remaining_gas: Gas,
        out: Bytes,
    ) -> (InstructionResult, Option<Address>, Gas, Bytes) {
        self.code_address.pop();
        self.state_address.pop();
        let proxy_sloads = self._proxy_sloads.pop().unwrap();
        let proxy_sstores = self._proxy_sstores.pop().unwrap();
        let implementation_sloads = self._implementation_sloads.pop().unwrap();
        let implementation_sstores = self._implementation_sstores.pop().unwrap();
        self._proxy_sloads.last_mut().unwrap().extend(proxy_sloads);
        self._proxy_sstores
            .last_mut()
            .unwrap()
            .extend(proxy_sstores);
        self._implementation_sloads
            .last_mut()
            .unwrap()
            .extend(implementation_sloads);
        self._implementation_sstores
            .last_mut()
            .unwrap()
            .extend(implementation_sstores);
        if let Some(address) = address {
            if address == self.proxy {
                self.proxy_created = true;
            }
        }
        (ret, address, remaining_gas, out)
    }
}

impl<S: BcState> EvmInspector<S> for StorageAccessInspector {
    fn transaction(&mut self, _tx: &libsofl_core::engine::types::TxEnv, _state: &S) -> bool {
        self._proxy_sloads.push(HashSet::new());
        self._proxy_sstores.push(HashSet::new());
        self._implementation_sloads.push(HashSet::new());
        self._implementation_sstores.push(HashSet::new());
        true
    }

    fn transaction_end(
        &mut self,
        _tx: &libsofl_core::engine::types::TxEnv,
        _state: &S,
        _result: &libsofl_core::engine::types::ExecutionResult,
    ) {
        assert!(self.code_address.is_empty());
        assert!(self.state_address.is_empty());
        self.proxy_sloads.extend(self._proxy_sloads.pop().unwrap());
        self.proxy_sstores
            .extend(self._proxy_sstores.pop().unwrap());
        self.implementation_sloads
            .extend(self._implementation_sloads.pop().unwrap());
        self.implementation_sstores
            .extend(self._implementation_sstores.pop().unwrap());
        assert!(self._proxy_sloads.is_empty());
        assert!(self._proxy_sstores.is_empty());
    }
}

#[cfg(test)]
mod tests {

    use std::collections::HashSet;

    use libsofl_core::{
        blockchain::{
            provider::{BcProvider, BcStateProvider},
            transaction::Tx,
        },
        conversion::ConvertTo,
        engine::{
            memory::MemoryBcState, state::BcState, transition::TransitionSpecBuilder, types::U256,
        },
    };
    use libsofl_reth::config::RethConfig;
    use libsofl_utils::{
        config::Config,
        solidity::{
            caller::HighLevelCaller,
            scripting::{deploy_contracts, SolScriptConfig},
        },
    };

    #[test]
    fn test_collision() {
        let mut state = MemoryBcState::fresh();
        let mut addrs = deploy_contracts(
            &mut state,
            "0.8.12",
            r#"
            contract Proxy {
                address public implementation;
                function set_implementation(address _impl) public {
                    implementation = _impl;
                }
                fallback() external payable {
                    address _impl = implementation;
                    assembly {
                        calldatacopy(0, 0, calldatasize())
                        let result := delegatecall(gas(), _impl, 0, calldatasize(), 0, 0)
                        returndatacopy(0, 0, returndatasize())
                        switch result
                        case 0 { revert(0, returndatasize()) }
                        default { return(0, returndatasize()) }
                    }
                }
            }
            contract Impl {
                uint256 public value;
                function set_value(uint256 _value) public {
                    value = _value;
                }
            }
            "#,
            vec!["Proxy", "Impl"],
            SolScriptConfig::default(),
        )
        .unwrap();
        let (proxy, implementation) = (addrs.remove(0), addrs.remove(0));
        let caller = HighLevelCaller::default().bypass_check();
        let mut insp = super::StorageAccessInspector::new(proxy, implementation, 0, 1, false);
        caller
            .invoke(
                &mut state,
                proxy,
                "set_implementation(address)",
                &[implementation.into()],
                None,
                &mut insp,
            )
            .unwrap();
        caller
            .invoke(
                &mut state,
                proxy,
                "set_value(uint256)",
                &[U256::from(1).into()],
                None,
                &mut insp,
            )
            .unwrap();
        let proxy_sstores = insp
            .proxy_sstores
            .iter()
            .map(|(a, k, _)| (a, k))
            .collect::<HashSet<_>>();
        let implementation_sstores = insp
            .implementation_sstores
            .iter()
            .map(|(a, k, _)| (a, k))
            .collect::<HashSet<_>>();
        assert!(proxy_sstores.intersection(&implementation_sstores).count() > 0);
    }

    #[test]
    fn test_corner_case() {
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let tx = provider
            .tx("0x05f4ac5047c448a05239b1ddbae5023a7e559ff388093ce15262ded696e3debe".cvt())
            .unwrap();
        let mut state = provider.bc_state_at(tx.position().unwrap()).unwrap();
        let spec = TransitionSpecBuilder::default()
            .at_block(&provider, tx.position().unwrap().block)
            .append_tx(tx)
            .build();
        let mut insp = super::StorageAccessInspector::new(
            "0x0cc7531ab6224ef21baa4afadc04a3315c3d3f69".cvt(),
            "0x8190799786cff757f5ab5d1d21b81fb342bf976c".cvt(),
            0,
            1,
            false,
        );
        state.transit(spec, &mut insp).unwrap();
    }

    #[test]
    fn test_comptroller() {
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let tx = provider
            .tx("0x2a0a38df1971eefe396a357ad2ba6fbcf931be4353cdfa7544bb1ad9455f55ec".cvt())
            .unwrap();
        let mut state = provider.bc_state_at(tx.position().unwrap()).unwrap();
        let spec = TransitionSpecBuilder::default()
            .at_block(&provider, tx.position().unwrap().block)
            .append_tx(tx)
            .build();
        let mut insp: crate::inspectors::collision::StorageAccessInspector =
            super::StorageAccessInspector::new(
                "0x5529CAefd3dE5C70Ab37Fb792fa9D622E11a2697".cvt(),
                "0xE16DB319d9dA7Ce40b666DD2E365a4b8B3C18217".cvt(),
                0,
                1,
                false,
            );
        state.transit(spec, &mut insp).unwrap();
        println!("{:?}", insp);
    }

    #[test]
    fn test_identify_proxy_creation_tx() {
        let provider = RethConfig::must_load().bc_provider().unwrap();
        let tx = provider
            .tx("0x5b9729630f0f6953ddea01d31185df0e2021465f53cf1c80fe4ee8a4d6abbbce".cvt())
            .unwrap();
        let mut state = provider.bc_state_at(tx.position().unwrap()).unwrap();
        let spec = TransitionSpecBuilder::default()
            .at_block(&provider, tx.position().unwrap().block)
            .append_tx(tx)
            .build();
        let mut insp: crate::inspectors::collision::StorageAccessInspector =
            super::StorageAccessInspector::new(
                "0x657669900034a3d773450038efa631616fae600c".cvt(),
                "0xe7e0ddc778d6332c449dc0789c1c8b7c1c6154c2".cvt(),
                0,
                1,
                false,
            );
        state.transit(spec, &mut insp).unwrap();
        assert_eq!(insp.proxy_created, true);
    }
}
