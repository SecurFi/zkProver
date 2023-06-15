use hashbrown::HashMap;
pub use revm::{
    EVM, primitives::{
        Env, result::ExecutionResult, TransactTo, KECCAK_EMPTY, 
        B160, B256, U256, Bytes
    },
};
use revm::{
    primitives:: {
        AccountInfo, Bytecode,
        State,
    },
    db::{Database, DatabaseRef}
};
use serde::{Deserialize, Serialize};

pub type StorageInfo = HashMap<U256, U256>;


#[derive(Default, Clone, Deserialize, Serialize)]
pub struct ZkDb {
    pub accounts: HashMap<B160, AccountInfo>,
    pub storage: HashMap<B160, StorageInfo>,
    pub block_hashes: HashMap<U256, B256>,
}

impl Database for ZkDb {
    type Error = ();

    /// Get basic account information.
    fn basic(&mut self, _address: B160) -> Result<Option<AccountInfo>, Self::Error> {
        let v = self.accounts.get(&_address).map(|x| x.clone());
        Ok(v)
    }

    /// Get account code by its hash
    fn code_by_hash(&mut self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        panic!()
    }

    /// Get storage value of address at index.
    fn storage(
        &mut self,
        _address: B160,
        _index: revm::primitives::U256,
    ) -> Result<revm::primitives::U256, Self::Error> {
        let v = self.storage.get(&_address).unwrap().get(&_index).unwrap().clone();
        Ok(v)
    }

    // History related
    fn block_hash(&mut self, _number: revm::primitives::U256) -> Result<B256, Self::Error> {
        let v = self.block_hashes.get(&_number).unwrap().clone();
        Ok(v)
    }
}


impl DatabaseRef for ZkDb {
    type Error = ();

     /// Get basic account information.
     fn basic(&self, _address: B160) -> Result<Option<AccountInfo>, Self::Error> {
        let v = self.accounts.get(&_address).map(|x| x.clone());
        Ok(v)
    }

    /// Get account code by its hash
    fn code_by_hash(&self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        panic!()
    }

    /// Get storage value of address at index.
    fn storage(
        &self,
        _address: B160,
        _index: revm::primitives::U256,
    ) -> Result<revm::primitives::U256, Self::Error> {
        match self.storage.get(&_address) {
            Some(storage) => {
                match storage.get(&_index) {
                    Some(v) => Ok(v.clone()),
                    None => Err(())
                }
            },
            None => Err(())
        }

    }

    // History related
    fn block_hash(&self, _number: revm::primitives::U256) -> Result<B256, Self::Error> {
        let v = self.block_hashes.get(&_number).unwrap().clone();
        Ok(v)
    }
}

#[derive(Deserialize, Serialize)]
pub struct EvmResult {
    pub env: Env,
    pub db: ZkDb,
    pub gas_used: u64,
    pub state: State,
}

