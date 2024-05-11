use alloy_primitives::{Address, B256, U256};
use alloy_provider::{Network, Provider};
use alloy_transport::Transport;

use anyhow::{Result, Context};
use bridge::{MemDB, AccountStorage};
use log::{debug, warn};
use std::collections::BTreeMap as Map;
use revm::primitives::{AccountInfo, Bytecode, SpecId};
pub use revm::{DatabaseRef, Database, DatabaseCommit};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::{fs, io::BufWriter, path::PathBuf};
use crate::block::BlockHeader;
use crate::utils::RuntimeOrHandle;


#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainSpec {
    pub chain_id: u64,
    pub spec_id: SpecId,
}

impl ChainSpec {
    pub fn mainnet() -> Self {
        Self { chain_id: 1, spec_id: SpecId::SHANGHAI }
    }
}


#[derive(Debug, Clone, Eq, Serialize, Deserialize)]
pub struct BlockchainDbMeta {
    pub chain_spec: ChainSpec,
    pub header: BlockHeader,
}

impl PartialEq for BlockchainDbMeta {
    fn eq(&self, other: &Self) -> bool {
        self.chain_spec == other.chain_spec && self.header == other.header
    }
}



#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Failed to get account for {0:?}: {0:?}")]
    GetAccount(Address, anyhow::Error),
    #[error("Failed to get storage for {0:?} at {1:?}: {2:?}")]
    GetStorage(Address, U256, anyhow::Error),
    #[error("Failed to get block hash for {0}: {1:?}")]
    GetBlockHash(u64, anyhow::Error),
    #[error(transparent)]
    Custom(#[from] anyhow::Error),
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonBlockCacheData {
    pub meta: BlockchainDbMeta,
    /// Account related data
    pub accounts: Map<Address, AccountInfo>,
    /// Storage related data
    pub storage: Map<Address, Map<U256, U256>>,
    /// All retrieved block hashes
    pub block_hashes: Map<u64, B256>,
}

/// A [JsonBlockCacheDB] that stores the cached content in a json file
#[derive(Debug)]
pub struct JsonBlockCacheDB<T: Transport + Clone, N: Network, P: Provider<T, N>> {
    /// The provider that's used to fetch data
    provider: P,
    /// The runtime that's used to run async tasks
    tokio_handle: RuntimeOrHandle,
    /// If this is a [None] then caching is disabled
    cache_path: Option<PathBuf>,
    /// Object that's stored in a json file
    data: RefCell<JsonBlockCacheData>,
    _marker: std::marker::PhantomData<fn() -> (T, N)>,
}

impl<T: Transport + Clone, N: Network, P: Provider<T, N>> JsonBlockCacheDB<T, N, P> {
    pub fn new(provider: P, meta: BlockchainDbMeta, cache_path: Option<PathBuf>) -> Self {
        let tokio_handle = RuntimeOrHandle::new();

        let cache = cache_path
            .as_ref()
            .and_then(|p| Self::load_cache(p).ok().filter(|cache| cache.meta == meta))
            .unwrap_or_else(|| JsonBlockCacheData {
                meta,
                accounts: Map::new(),
                storage: Map::new(),
                block_hashes: Map::new(),
            });
        
        Self {
            provider,
            tokio_handle,
            cache_path,
            data: RefCell::new(cache),
            _marker: std::marker::PhantomData,
        }
    }

    fn load_cache(path: impl Into<PathBuf>) -> Result<JsonBlockCacheData> {
        let path = path.into();
        debug!("{:?}, reading json cache", path);
        let file = fs::File::open(&path).map_err(|err| {
            warn!("{:?}, {:?}, Failed to read cache file",err, path);
            err
        })?;
        let file = std::io::BufReader::new(file);
        let data: JsonBlockCacheData = serde_json::from_reader(file).map_err(|err| {
            warn!("{:?}, {:?}, Failed to deserialize cache data", err, path);
            err
        })?;
        Ok(data)
    }

    /// Returns `true` if this is a transient cache and nothing will be flushed
    pub fn is_transient(&self) -> bool {
        self.cache_path.is_none()
    }

    /// Flushes the DB to disk if caching is enabled
    pub fn flush(&self) {
        // writes the data to a json file
        if let Some(ref path) = self.cache_path {
            debug!("saving json cache path={:?}", path);
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::File::create(path)
                .map_err(|e| warn!("Failed to open json cache for writing: {}", e))
                .and_then(|f| {
                    serde_json::to_writer(BufWriter::new(f), &self.data)
                        .map_err(|e| warn!(target: "cache" ,"Failed to write to json cache: {}", e))
                });
                debug!("saved json cache path={:?}", path);
        }
    }

    pub fn data(&self) -> JsonBlockCacheData{
        self.data.borrow().clone()
    }

}

impl<T: Transport + Clone, N: Network, P: Provider<T, N>> Drop for JsonBlockCacheDB<T, N, P> {
    fn drop(&mut self) {
        self.flush();
    }
}


impl<T: Transport + Clone, N: Network, P: Provider<T, N>> DatabaseRef for JsonBlockCacheDB<T, N, P>
{
    type Error = DbError;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {        
        match self.data.borrow().accounts.get(&address) {
            Some(account) => return Ok(Some(account.clone())),
            None => {}
        }
        debug!("Fetching account {} from rpc", address);
        let block_id = self.data.borrow().meta.header.number.into();
        let (balance, nonce, code) = self
            .tokio_handle
            .block_on(async {
                let balance = self.provider.get_balance(address, block_id);
                let nonce = self.provider.get_transaction_count(address, block_id);
                let code = self.provider.get_code_at(address, block_id);
                tokio::try_join!(balance, nonce, code)
            })
            .map_err(|err| DbError::GetAccount(address, anyhow::Error::new(err)))?;
        let bytecode = Bytecode::new_raw(code);
        let account_info = AccountInfo::new(
            balance,
            nonce,
            bytecode.hash_slow(),
            bytecode,
        );
        self.data
            .borrow_mut()
            .accounts
            .insert(address, account_info.clone());
        Ok(Some(account_info))
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {        
        let value = self
            .data
            .borrow()
            .storage
            .get(&address)
            .and_then(|s| s.get(&index).copied());
        if let Some(value) = value {
            return Ok(value);
        }
        debug!("Fetching storage {} {} from rpc", address, index);
        let block_id = self.data.borrow().meta.header.number.into();
        let data = self
            .tokio_handle
            .block_on(async {
                let storage = self
                    .provider
                    .get_storage_at(address, index, block_id)
                    .await;
                storage
            })
            .map_err(|err| DbError::GetStorage(address, index, anyhow::Error::new(err)))?;
        self.data
            .borrow_mut()
            .storage
            .entry(address)
            .or_default()
            .insert(index, data);
        Ok(data)
    }

    fn block_hash_ref(&self, number: U256) -> Result<B256, Self::Error> {
        let block_number = u64::try_from(number).unwrap();
        match self.data.borrow().block_hashes.get(&block_number) {
            Some(hash) => return Ok(*hash),
            None => {}
        }
        debug!("Fetching block hash {} from rpc", number);
        let block = self
            .tokio_handle
            .block_on(async {
                let block = self.provider.get_block(block_number.into(), false).await;
                block
            })
            .map_err(|err| DbError::GetBlockHash(block_number, anyhow::Error::new(err)))?;
        let block = block.context("block not found")?;
        let hash = block.header.hash.context("block hash not found")?;
        self.data
            .borrow_mut()
            .block_hashes
            .insert(block_number, hash);
        Ok(hash)
    }

    fn code_by_hash_ref(&self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        unreachable!()
    }
}


pub struct ProxyDB<ExtDB> {
    pub hook_accounts: Map<Address, AccountInfo>,
    pub hook_storage: Map<Address, Map<U256, U256>>,
    pub db: ExtDB,
    pub trace_basic: Vec<Address>,
    pub trace_storage: Vec<(Address, U256)>,
    pub trace_block_hashes: Vec<U256>,
}

impl<ExtDB> ProxyDB<ExtDB> {
    pub fn new(db: ExtDB) -> Self {
        Self {
            hook_accounts: Map::default(),
            hook_storage: Map::default(),
            db,
            trace_basic: Vec::default(),
            trace_storage: Vec::default(),
            trace_block_hashes: Vec::default(),
        }
    }

    pub fn insert_account_info(&mut self, address: Address, info: AccountInfo) {
        self.hook_accounts.insert(address, info);
    }

    pub fn insert_account_storage(&mut self, address: Address, index: U256, value: U256) {
        self.hook_storage
           .entry(address)
           .or_default()
           .insert(index, value);
    }

}


impl<ExtDB: DatabaseRef> Database for ProxyDB<ExtDB> {
    type Error = ExtDB::Error;

    fn basic(&mut self, address:Address) -> Result<Option<AccountInfo> ,Self::Error> {
        self.trace_basic.push(address);
        <Self as DatabaseRef>::basic_ref(self, address)
    }

    #[inline]
    fn code_by_hash(&mut self, code_hash:B256) -> Result<Bytecode,Self::Error> {
        <Self as DatabaseRef>::code_by_hash_ref(self, code_hash)
    }


    fn storage(&mut self, address:Address, index:U256) -> Result<U256,Self::Error> {
        self.trace_storage.push((address, index));
        <Self as DatabaseRef>::storage_ref(self, address, index)
    }

    fn block_hash(&mut self, number:U256) -> Result<B256,Self::Error> {
        self.trace_block_hashes.push(number);
        <Self as DatabaseRef>::block_hash_ref(self, number)
    }

}

impl <ExtDB: DatabaseRef> DatabaseRef for ProxyDB<ExtDB> {
    type Error = ExtDB::Error;
    
    fn basic_ref(&self, address:Address) -> Result<Option<AccountInfo> ,Self::Error>  {
        match self.hook_accounts.get(&address) {
            Some(info) => {
                self.db.basic_ref(address)?;
                Ok(Some(info.clone()))
            },
            None => self.db.basic_ref(address)
        }
    }
    
    fn code_by_hash_ref(&self, _code_hash:B256) -> Result<Bytecode,Self::Error>  {
        todo!()
    }
    
    fn storage_ref(&self,address:Address,index:U256) -> Result<U256,Self::Error>  {
        match self.hook_storage.get(&address).and_then(|s| s.get(&index)) {
            Some(value) => {
                self.db.storage_ref(address, index)?;
                Ok(*value)
            },
            None => self.db.storage_ref(address, index)
        }
    }
    
    fn block_hash_ref(&self,number:U256) -> Result<B256,Self::Error>  {
        self.db.block_hash_ref(number)
    }

    
}



impl <ExtDB: DatabaseRef> ProxyDB<ExtDB> 
where <ExtDB as DatabaseRef>::Error: std::fmt::Debug
{
    pub fn into_memdb(&self) -> MemDB {
        let mut accounts: Map<Address, AccountStorage> = Map::new();
        let mut block_hashes: Vec<(u64, B256)> = vec![];

        for (address, slot) in self.trace_storage.iter() {
            let slot_value = self.storage_ref(address.clone(), slot.clone()).unwrap();

            match accounts.get_mut(address) {
                Some(account) => {
                    
                    account.storage.insert(*slot, slot_value);
                }
                None => {
                    let info = self.basic_ref(address.clone()).unwrap().unwrap();
                    let account = AccountStorage {
                        info: info,
                        storage: Map::new(),
                    };
                    accounts.insert(address.clone(), account);
                    accounts.get_mut(address).unwrap().storage.insert(*slot, slot_value);
                },
                
            }
        }
        for address in self.trace_basic.iter() {
            match accounts.get(address) {
                Some(_) => {},
                None => {
                    let info = self.basic_ref(address.clone()).unwrap().unwrap();
                    let account = AccountStorage {
                        info: info,
                        storage: Map::new(),
                    };
                    accounts.insert(address.clone(), account);
                },
            }
        }

        for block_number in self.trace_block_hashes.iter() {
            let block_hash = self.block_hash_ref(block_number.clone()).unwrap();
            block_hashes.push((block_number.clone().try_into().unwrap(), block_hash));
        }
        MemDB { accounts, block_hashes}
    }
}