//! A revm database that forks off a remote client

use crate::{
    // executor::{
        // backend::{error::DatabaseError, snapshot::StateSnapshot},
    error::{DatabaseError, DatabaseResult},
    fork::{BlockchainDb, SharedBackend},
        // snapshot::Snapshots,
    // },
};
pub use revm::db::CacheDB;
pub use revm::{DatabaseCommit, db::DatabaseRef, Database};
use ethers::{types::{BlockId, Address, U256}};
use hashbrown::HashMap as Map;
use revm::{
    primitives::{Account, AccountInfo, Bytecode, B160, B256, U256 as rU256},
};
use tracing::{trace};

/// a [revm::Database] that's forked off another client
///
/// The `backend` is used to retrieve (missing) data, which is then fetched from the remote
/// endpoint. The inner in-memory database holds this storage and will be used for write operations.
/// This database uses the `backend` for read and the `db` for write operations. But note the
/// `backend` will also write (missing) data to the `db` in the background
#[derive(Debug, Clone)]
pub struct ForkedDatabase {
    /// responsible for fetching missing data
    ///
    /// This is responsible for getting data
    backend: SharedBackend,
    /// Cached Database layer, ensures that changes are not written to the database that
    /// exclusively stores the state of the remote client.
    ///
    /// This separates Read/Write operations
    ///   - reads from the `SharedBackend as DatabaseRef` writes to the internal cache storage
    cache_db: CacheDB<SharedBackend>,
    /// Contains all the data already fetched
    ///
    /// This exclusively stores the _unchanged_ remote client state
    db: BlockchainDb,
}

impl ForkedDatabase {
    /// Creates a new instance of this DB
    pub fn new(backend: SharedBackend, db: BlockchainDb) -> Self {
        Self {
            cache_db: CacheDB::new(backend.clone()),
            backend,
            db,
        }
    }

    pub fn database(&self) -> &CacheDB<SharedBackend> {
        &self.cache_db
    }

    pub fn database_mut(&mut self) -> &mut CacheDB<SharedBackend> {
        &mut self.cache_db
    }

    /// Reset the fork to a fresh forked state, and optionally update the fork config
    pub fn reset(
        &mut self,
        _url: Option<String>,
        block_number: impl Into<BlockId>,
    ) -> Result<(), String> {
        self.backend.set_pinned_block(block_number).map_err(|err| err.to_string())?;

        // TODO need to find a way to update generic provider via url

        // wipe the storage retrieved from remote
        self.inner().db().clear();
        // create a fresh `CacheDB`, effectively wiping modified state
        self.cache_db = CacheDB::new(self.backend.clone());
        trace!(target: "backend::forkdb", "Cleared database");
        Ok(())
    }

    /// Flushes the cache to disk if configured
    pub fn flush_cache(&self) {
        self.db.cache().flush()
    }

    /// Returns the database that holds the remote state
    pub fn inner(&self) -> &BlockchainDb {
        &self.db
    }

}

impl Database for ForkedDatabase {
    type Error = DatabaseError;

    fn basic(&mut self, address: B160) -> Result<Option<AccountInfo>, Self::Error> {
        // Note: this will always return Some, since the `SharedBackend` will always load the
        // account, this differs from `<CacheDB as Database>::basic`, See also
        // [MemDb::ensure_loaded](crate::executor::backend::MemDb::ensure_loaded)
        Database::basic(&mut self.cache_db, address)
    }

    fn code_by_hash(&mut self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        Database::code_by_hash(&mut self.cache_db, code_hash)
    }

    fn storage(&mut self, address: B160, index: rU256) -> Result<rU256, Self::Error> {
        Database::storage(&mut self.cache_db, address, index)
    }

    fn block_hash(&mut self, number: rU256) -> Result<B256, Self::Error> {
        Database::block_hash(&mut self.cache_db, number)
    }
}

impl DatabaseRef for ForkedDatabase {
    type Error = DatabaseError;

    fn basic(&self, address: B160) -> Result<Option<AccountInfo>, Self::Error> {
        self.cache_db.basic(address)
    }

    fn code_by_hash(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.cache_db.code_by_hash(code_hash)
    }

    fn storage(&self, address: B160, index: rU256) -> Result<rU256, Self::Error> {
        DatabaseRef::storage(&self.cache_db, address, index)
    }

    fn block_hash(&self, number: rU256) -> Result<B256, Self::Error> {
        self.cache_db.block_hash(number)
    }
}

impl DatabaseCommit for ForkedDatabase {
    fn commit(&mut self, changes: Map<B160, Account>) {
        self.database_mut().commit(changes)
    }
}


pub trait Db: Database<Error = DatabaseError>
{
    /// Inserts an account
    fn insert_account(&mut self, address: Address, account: AccountInfo);
    /// Sets the balance of the given address
    fn set_storage_at(&mut self, address: Address, slot: U256, val: U256) -> DatabaseResult<()>;

}

impl Db for ForkedDatabase {
    fn insert_account(&mut self, address: Address, account: AccountInfo) {
        self.database_mut().insert_account_info(address.into(), account)
    }

    fn set_storage_at(&mut self, address: Address, slot: U256, val: U256) -> DatabaseResult<()> {
        self.database_mut().insert_account_storage(address.into(), slot.into(), val.into())
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::fork::BlockchainDbMeta;
    use crate::provider::get_http_provider;
    use std::collections::BTreeSet;
    use std::sync::Arc;

    /// Demonstrates that `Database::basic` for `ForkedDatabase` will always return the
    /// `AccountInfo`
    #[tokio::test(flavor = "multi_thread")]
    async fn fork_db_insert_basic_default() {
        let rpc = "https://rpc.flashbots.net/".to_string();
        let provider = get_http_provider(rpc.clone());
        let meta = BlockchainDbMeta {
            cfg_env: Default::default(),
            block_env: Default::default(),
            hosts: BTreeSet::from([rpc]),
        };
        let db = BlockchainDb::new(meta, None);

        let backend = SharedBackend::spawn_backend(Arc::new(provider), db.clone(), None).await;

        let mut db = ForkedDatabase::new(backend, db);
        let address = B160::random();

        let info = Database::basic(&mut db, address).unwrap();
        assert!(info.is_some());
        let mut info = info.unwrap();
        info.balance = rU256::from(500u64);

        // insert the modified account info
        db.database_mut().insert_account_info(address, info.clone());

        let loaded = Database::basic(&mut db, address).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap(), info);
    }
}
