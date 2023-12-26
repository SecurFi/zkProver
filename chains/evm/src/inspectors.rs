use alloy_primitives::{address, Address, Bytes, U256};
use alloy_sol_types::{sol, SolInterface, SolValue};
use std::fmt;
use std::collections::BTreeMap;
use revm::{
    interpreter::{opcode, CallInputs, Gas, InstructionResult, Interpreter},
    primitives::Account,
    Database, EVMData, Inspector,
};

/// `address(bytes20(uint160(uint256(keccak256('hevm cheat code')))))`
pub const CHEATCODE_ADDRESS: Address = address!("7109709ECfa91a80626fF3989D68f67F5b1DD12D");

sol! {
#[derive(Debug)]
interface Vm {
    function load(address target, bytes32 slot) external view returns (bytes32 data);
    function deal(address account, uint256 newBalance) external;
    function store(address target, bytes32 slot, bytes32 value) external;
    function record() external;
    function accesses(address target) external returns (bytes32[] memory readSlots, bytes32[] memory writeSlots);
}
}


#[derive(Clone, Debug, Default)]
pub struct RecordAccess {
    pub reads: BTreeMap<Address, Vec<U256>>,
    pub writes: BTreeMap<Address, Vec<U256>>,
}

pub struct CheatCodesInspector {
    pub accesses: Option<RecordAccess>,
}

impl CheatCodesInspector {
    pub fn new() -> Self {
        Self {
            accesses: Some(RecordAccess::default()),
        }
    }

    fn apply_cheatcode<DB: Database>(
        &mut self,
        data: &mut EVMData<'_, DB>,
        call: &CallInputs,
    ) -> Result<Vec<u8>, Bytes> {
        let decoded = Vm::VmCalls::abi_decode(&call.input, false).map_err(error_to_bytes)?;
        // let caller = call.context.caller;

        let res = match decoded {
            Vm::VmCalls::store(inner) => {
                let _ = journaled_account(data, inner.target).map_err(|_err| Bytes::from("db error"))?;
                data.journaled_state.sstore(
                    inner.target,
                    inner.slot.into(),
                    inner.value.into(),
                    data.db,
                ).map_err(|_err| Bytes::from("sstore error"))?;
                Default::default()
            }
            Vm::VmCalls::load(inner) => {
                data.journaled_state.load_account(inner.target, data.db).map_err(|_err| Bytes::from("db error"))?;
                let (val, _) = data.journaled_state.sload(inner.target, inner.slot.into(), data.db).map_err(|_err| Bytes::from("db error"))?;
                val.abi_encode()
            }

            Vm::VmCalls::deal(inner) => {
                let account = journaled_account(data, inner.account).map_err(|_err| Bytes::from("db error"))?;
                account.info.balance = inner.newBalance;
                Default::default()
            }

            Vm::VmCalls::record(_) => {
                self.accesses = Some(RecordAccess::default());
                Default::default()
            }

            Vm::VmCalls::accesses(inner) => {
                let address = inner.target;
                let result = self.accesses.as_mut().map(|accesses| {
                    (
                        &accesses.reads.entry(address).or_default()[..],
                        &accesses.writes.entry(address).or_default()[..],
                    )
                }).unwrap_or_default();
                result.abi_encode_params()
            }

        };
        Ok(res)
    }
}

macro_rules! try_or_continue {
    ($e:expr) => {
        match $e {
            Ok(v) => v,
            Err(_) => return InstructionResult::Continue,
        }
    };
}

impl<DB> Inspector<DB> for CheatCodesInspector
where
    DB: Database,
{
    fn step(
        &mut self,
        interpreter: &mut Interpreter,
        _data: &mut EVMData<'_, DB>,
    ) -> InstructionResult {
     
        if let Some(storage_accesses) = &mut self.accesses {
            match interpreter.current_opcode() {
                opcode::SLOAD => {
                    let key = try_or_continue!(interpreter.stack().peek(0));
                    storage_accesses
                        .reads
                        .entry(interpreter.contract().address)
                        .or_default()
                        .push(key);
                }
                opcode::SSTORE => {
                    let key = try_or_continue!(interpreter.stack().peek(0));
                    storage_accesses
                        .reads
                        .entry(interpreter.contract().address)
                        .or_default()
                        .push(key);
                    storage_accesses
                        .writes
                        .entry(interpreter.contract().address)
                        .or_default()
                        .push(key);
                }
                _ => (),
            }
        }

        InstructionResult::Continue
    }

    fn call(
        &mut self,
        data: &mut EVMData<'_, DB>,
        call: &mut CallInputs,
    ) -> (InstructionResult, Gas, Bytes) {
        let gas = Gas::new(call.gas_limit);
        if call.contract == CHEATCODE_ADDRESS {
            match self.apply_cheatcode(data, call) {
                Ok(retdata) => (InstructionResult::Return, gas, retdata.into()),
                Err(err) => (InstructionResult::Revert, gas, err),
            }
        } else {
            (
                InstructionResult::Continue,
                gas,
                Bytes::new(),
            )
        }
    }
}

pub(super) fn journaled_account<'a, DB: Database>(
    data: &'a mut EVMData<'_, DB>,
    addr: Address,
) -> Result<&'a mut Account, DB::Error> {
    data.journaled_state.load_account(addr, data.db)?;
    data.journaled_state.touch(&addr);
    Ok(data
        .journaled_state
        .state
        .get_mut(&addr)
        .expect("account is loaded"))
}

fn error_to_bytes(err: impl fmt::Display) -> Bytes {
    fmt::format(format_args!("{err}")).into()
}