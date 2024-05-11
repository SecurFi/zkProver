use std::collections::BTreeMap as Map;
use alloy_primitives::{address, bytes, Bytes};
use revm::{
    db::DatabaseRef, primitives:: {
        AccountInfo, Address, Bytecode, ResultAndState, SpecId, State, TransactTo, B256, U256,
        BlockEnv
    }, Evm
};
use serde::{Deserialize, Serialize};


#[derive(Default, Clone, Deserialize, Serialize)]
pub struct AccountStorage {
    pub info: AccountInfo,
    pub storage: Map<U256, U256>,
}


#[derive(Default, Clone, Deserialize, Serialize)]
pub struct MemDB {
    pub accounts: Map<Address, AccountStorage>,
    pub block_hashes: Vec<(u64, B256)>,
}


impl DatabaseRef for MemDB {
    type Error = ();

     /// Get basic account information.
     fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        match self.accounts.get(&address) {
            Some(db_account) => {
                Ok(Some(db_account.info.clone()))
            },
            None => {
                Err(())
            }
        }
    }

    /// Get account code by its hash
    fn code_by_hash_ref(&self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        panic!()
    }

    /// Get storage value of address at index.
    fn storage_ref(
        &self,
        address: Address,
        index: U256,
    ) -> Result<U256, Self::Error> {
        match self.accounts.get(&address) {
            Some(db_account) => match db_account.storage.get(&index) {
                Some(value) => {
                    Ok(*value)
                },
                None => {
                    Err(())
                }
            },
            None => {
                Err(())
            }
        }

    }

    // History related
    fn block_hash_ref(&self, number: U256) -> Result<B256, Self::Error> {
        let block_no: u64 = number.try_into().unwrap();
        let entry = self.block_hashes.iter()
            .find(|(k, _)| *k == block_no);
        match entry {
            Some((_, v)) => Ok(*v),
            None => {
                Err(())
            }
        }
    }
}

/// The address was derived from `address(uint160(uint256(keccak256("0xhacked default caller"))))`
/// and is equal to 0xe42a4fc3902506f15E7E8FC100542D6310d1c93a.
pub const DEFAULT_CALLER: Address = address!("e42a4fc3902506f15E7E8FC100542D6310d1c93a");

/// Stores the default poc contract address: 0x412049F92065a2597458c4cE9b969C846fE994fD
pub const DEFAULT_CONTRACT_ADDRESS: Address = address!("412049F92065a2597458c4cE9b969C846fE994fD");

/// func exploit()
pub const CALL_EXPLOIT_DATA: Bytes = bytes!("63d9b770");

pub const DEFAULT_GAS_LIMIT: u64 = 15_000_000;


#[derive(Deserialize, Serialize)]
pub struct ExploitInput {
    pub db: MemDB,
    pub block_env: BlockEnv,
    pub spec_id: SpecId, 
}


#[derive(Deserialize, Serialize)]
pub struct ExploitOutput {
    pub input: ExploitInput,
    pub gas_used: u64,
    pub state: State,
}

pub fn sim_exploit(input: &ExploitInput) -> ResultAndState {
    let mut evm = Evm::builder()
        .with_ref_db(&input.db)
        .with_spec_id(input.spec_id)
        .with_block_env(input.block_env.clone())
        .modify_tx_env(|tx| {
            tx.caller = DEFAULT_CALLER;
            tx.transact_to = TransactTo::Call(DEFAULT_CONTRACT_ADDRESS);
            tx.data = CALL_EXPLOIT_DATA;
            tx.value = U256::ZERO;
            tx.gas_limit = DEFAULT_GAS_LIMIT;
        })
        .build();

    evm.transact().unwrap()
}