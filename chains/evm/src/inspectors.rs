use std::collections::BTreeMap;
use ethers::{
    contract::abigen,
    types::{Address, U256, H160},
    abi::{AbiDecode, AbiEncode, Token, Tokenize},
};
use bytes::Bytes;
use revm::{
    interpreter::{Interpreter, opcode, CallInputs, InstructionResult, Gas},
    Inspector, Database,
    EVMData, JournaledState,
    primitives::{Account}
};
use tracing::trace;

use crate::utils::{b160_to_h160, h160_to_b160, u256_to_ru256, ru256_to_u256};

/// `address(bytes20(uint160(uint256(keccak256('hevm cheat code')))))`
pub const CHEATCODE_ADDRESS: Address = H160([
    0x71, 0x09, 0x70, 0x9E, 0xcf, 0xa9, 0x1a, 0x80, 0x62, 0x6f, 0xf3, 0x98, 0x9d, 0x68, 0xf6, 0x7f,
    0x5b, 0x1d, 0xd1, 0x2d,
]);

abigen!(
    HEVM,
    "[
        store(address,bytes32,bytes32)
        load(address,bytes32)(bytes32)
        deal(address,uint256)
        record()
        accesses(address)(bytes32[],bytes32[])
    ]",
);
pub use hevm::{HEVMCalls, HEVM_ABI};

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
        data: &mut EVMData<'_,DB> ,
        _caller: Address,
        call: &CallInputs,
    ) -> Result<Bytes, Bytes> {
        let decoded = HEVMCalls::decode(&call.input).map_err(|err| err.to_string().encode())?;
        
        let res = match decoded {
            HEVMCalls::Store(inner) => {
                data.journaled_state
                .load_account(h160_to_b160(inner.0), data.db)
                .map_err(|_err| Bytes::from("db error".encode()))?;
                // ensure the account is touched
                data.journaled_state.touch(&h160_to_b160(inner.0));

                data.journaled_state
                    .sstore(
                        h160_to_b160(inner.0),
                        u256_to_ru256(inner.1.into()),
                        u256_to_ru256(inner.2.into()),
                        data.db,
                    )
                    .map_err(|_err| Bytes::from("db error".encode()))?;
                Bytes::new()
            }
            HEVMCalls::Load(inner) => {
                data.journaled_state
                    .load_account(h160_to_b160(inner.0), data.db)
                    .map_err(|_err| Bytes::from("db error".encode()))?;
                let (val, _) = data
                    .journaled_state
                    .sload(h160_to_b160(inner.0), u256_to_ru256(inner.1.into()), data.db)
                    .map_err(|_err| Bytes::from("db error".encode()))?;
                ru256_to_u256(val).encode().into()
            }
            HEVMCalls::Deal(inner) => {
                let who = inner.0;
                let value = inner.1;
                trace!(?who, ?value, "deal cheatcode");
                with_journaled_account(&mut data.journaled_state, data.db, who, |account| {
  
                    account.info.balance = value.into();
                })
                .map_err(|_err| Bytes::from("db error".encode()))?;
                Bytes::new()
            }
            HEVMCalls::Record(_) => {
                self.accesses = Some(Default::default());
                Bytes::new()
            }
            HEVMCalls::Accesses(inner) => {
                let address = inner.0;
                if let Some(storage_accesses) = &mut self.accesses {
                    ethers::abi::encode(&[
                        storage_accesses.reads.remove(&address).unwrap_or_default().into_tokens()[0].clone(),
                        storage_accesses.writes.remove(&address).unwrap_or_default().into_tokens()[0].clone(),
                    ])
                    .into()
                } else {
                    ethers::abi::encode(&[Token::Array(vec![]), Token::Array(vec![])]).into()
                }
            }
            _ => return Err("invalid cheatcode".into())
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

impl <DB> Inspector<DB> for CheatCodesInspector
where
    DB: Database,
{
    fn step(
        &mut self,
        interpreter: &mut Interpreter,
        _data: &mut EVMData<'_,DB> ,
        _is_static:bool,
    ) -> InstructionResult {
        let pc = interpreter.program_counter();
        if let Some(storage_accesses) = &mut self.accesses {
            match interpreter.contract.bytecode.bytecode()[pc] {
                opcode::SLOAD => {
                    let key = try_or_continue!(interpreter.stack().peek(0));
                    storage_accesses
                        .reads
                        .entry(b160_to_h160(interpreter.contract().address))
                        .or_insert_with(Vec::new)
                        .push(key.into());
                }
                opcode::SSTORE => {
                    let key = try_or_continue!(interpreter.stack().peek(0));
                    storage_accesses
                        .reads
                        .entry(b160_to_h160(interpreter.contract().address))
                        .or_insert_with(Vec::new)
                        .push(key.into());
                    storage_accesses
                        .writes
                        .entry(b160_to_h160(interpreter.contract().address))
                        .or_insert_with(Vec::new)
                        .push(key.into());
                }
                _ => {
                    // println!("other opcode: {:?}", opcode::OPCODE_JUMPMAP[interpreter.contract.bytecode.bytecode()[pc] as usize]);
                },
            }
        }

        InstructionResult::Continue
    }

    fn call(
        &mut self,
        data: &mut EVMData<'_,DB> ,
        call: &mut CallInputs,
        _is_static:bool,
    ) -> (InstructionResult, Gas, Bytes) {
        if call.contract == h160_to_b160(CHEATCODE_ADDRESS) {
            match self.apply_cheatcode(data, b160_to_h160(call.context.caller), call) {
                Ok(retdata) => (InstructionResult::Return, Gas::new(call.gas_limit), retdata),
                Err(err) => (InstructionResult::Revert, Gas::new(call.gas_limit), err),
            }
        } else {
            (InstructionResult::Continue, Gas::new(call.gas_limit), Bytes::new())
        }
    }
}

pub fn with_journaled_account<F, R, DB: Database>(
    journaled_state: &mut JournaledState,
    db: &mut DB,
    addr: Address,
    mut f: F,
) -> Result<R, DB::Error>
where
    F: FnMut(&mut Account) -> R,
{
    let addr = h160_to_b160(addr);
    journaled_state.load_account(addr, db)?;
    journaled_state.touch(&addr);
    let account = journaled_state.state.get_mut(&addr).expect("account loaded;");
    Ok(f(account))
}
