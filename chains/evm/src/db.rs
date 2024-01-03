use bridge::{BlockHeader, StorageEntry, DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER};
use bridge::trie::{MptNode, };
use ethers_providers::Middleware;
use ethers_core::types::{H256, Bytes as EBytes, StorageProof};
use eyre::{Result, Context, ContextCompat};
use log::{debug, warn};
use std::collections::BTreeMap as Map;
use hashbrown::{HashMap, HashSet};
use revm::primitives::{AccountInfo, Bytecode, SpecId};
pub use revm::{db::DatabaseRef, Database, DatabaseCommit};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::{fs, io::BufWriter, path::PathBuf};
use crate::evm_primitives::{Address, B256, U256, ToAlloy, ToEthers, Bytes};
use crate::mpt::{parse_proof, mpt_from_proof, resolve_nodes, node_from_digest};
use crate::utils::{RuntimeOrHandle};


#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChainSpec {
    pub chain_id: U256,
    pub spec_id: SpecId,
}

impl ChainSpec {
    pub fn mainnet() -> Self {
        Self { chain_id: U256::from(1), spec_id: SpecId::SHANGHAI }
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
    GetAccount(Address, eyre::Error),
    #[error("Failed to get storage for {0:?} at {1:?}: {2:?}")]
    GetStorage(Address, U256, eyre::Error),
    #[error("Failed to get block hash for {0}: {1:?}")]
    GetBlockHash(u64, eyre::Error),
    #[error(transparent)]
    Custom(#[from] eyre::Error),
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AccountProof {
    pub storage_hash: H256,
    pub account_proof: Vec<EBytes>,
    pub storage_proof: Vec<StorageProof>,
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
    /// All proofs
    pub account_proofs: Map<Address, AccountProof>,
}

/// A [JsonBlockCacheDB] that stores the cached content in a json file
#[derive(Debug)]
pub struct JsonBlockCacheDB<'a, M: Middleware> {
    /// The provider that's used to fetch data
    provider: &'a M,
    /// The runtime that's used to run async tasks
    tokio_handle: RuntimeOrHandle,
    /// If this is a [None] then caching is disabled
    cache_path: Option<PathBuf>,
    /// Object that's stored in a json file
    data: RefCell<JsonBlockCacheData>,
}

impl<'a, M: Middleware + 'static> JsonBlockCacheDB<'a, M> {
    pub fn new(provider: &'a M, meta: BlockchainDbMeta, cache_path: Option<PathBuf>) -> Self {
        let tokio_handle = RuntimeOrHandle::new();

        let cache = cache_path
            .as_ref()
            .and_then(|p| Self::load_cache(p).ok().filter(|cache| cache.meta == meta))
            .unwrap_or_else(|| JsonBlockCacheData {
                meta,
                accounts: Map::new(),
                storage: Map::new(),
                block_hashes: Map::new(),
                account_proofs: Map::new(),
            });
        Self {
            provider,
            tokio_handle,
            cache_path,
            data: RefCell::new(cache),
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

    pub fn get_proof(&self, address: Address, indices: &[U256]) -> Result<AccountProof> {
        let locations: Vec<H256> = indices.iter().map(|k| k.to_be_bytes().into()).collect::<Vec<_>>();

        let mut cache_data = self.data.borrow_mut();
        let block_id = Some(cache_data.meta.header.number.into());

        let keys_in_cache = cache_data.account_proofs.get(&address)
                .map(|p| p.storage_proof.iter().map(|s| s.key).collect::<Vec<_>>()).unwrap_or_default();
        let keys_not_in_cache = locations.iter().cloned().filter(|k| !keys_in_cache.contains(k)).collect::<Vec<_>>();

        if keys_not_in_cache.len() > 0 || !cache_data.account_proofs.contains_key(&address) {
            // println!("missing storage proof for account: {}", address);
            let resp = self.tokio_handle.block_on(async {
                self.provider.get_proof(address.to_ethers(), keys_not_in_cache, block_id).await
            })?;

            let entry = cache_data.account_proofs.entry(address.clone()).or_default();
            entry.account_proof = resp.account_proof;
            entry.storage_hash = resp.storage_hash;
            for row in resp.storage_proof {
                entry.storage_proof.push(row);
            }
        }
        let cache_proof = cache_data.account_proofs.get(&address).expect("account proof not found");

        let storage_proof = cache_proof.storage_proof.iter().cloned().filter(|s| locations.contains(&s.key)).collect();
        Ok(
            AccountProof {
                storage_hash: cache_proof.storage_hash,
                account_proof: cache_proof.account_proof.clone(),
                storage_proof,
            }
        )
    }
}

impl<'a, M: Middleware> DatabaseRef for JsonBlockCacheDB<'a, M>
where
    M::Error: 'static,
{
    type Error = DbError;
    fn basic(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {        
        match self.data.borrow().accounts.get(&address) {
            Some(account) => return Ok(Some(account.clone())),
            None => {}
        }
        debug!("Fetching account {} from rpc", address);
        let block_id = Some(self.data.borrow().meta.header.number.into());
        let (balance, nonce, code) = self
            .tokio_handle
            .block_on(async {
                let address = address.to_ethers();
                let balance = self.provider.get_balance(address, block_id);
                let nonce = self.provider.get_transaction_count(address, block_id);
                let code = self.provider.get_code(address, block_id);
                tokio::try_join!(balance, nonce, code)
            })
            .map_err(|err| DbError::GetAccount(address, eyre::Error::new(err)))?;
        let bytecode = Bytecode::new_raw(code.0.into());
        let account_info = AccountInfo::new(
            balance.to_alloy(),
            nonce.as_u64(),
            bytecode.hash_slow(),
            bytecode,
        );
        self.data
            .borrow_mut()
            .accounts
            .insert(address, account_info.clone());
        Ok(Some(account_info))
    }

    fn storage(&self, address: Address, index: U256) -> Result<U256, Self::Error> {        
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
        let block_id = Some(self.data.borrow().meta.header.number.into());
        let data = self
            .tokio_handle
            .block_on(async {
                let storage = self
                    .provider
                    .get_storage_at(address.to_ethers(), index.to_be_bytes().into(), block_id)
                    .await;
                let storage = storage.map(|v| U256::from_be_bytes(v.to_fixed_bytes()));
                storage
            })
            .map_err(|err| DbError::GetStorage(address, index, eyre::Error::new(err)))?;
        self.data
            .borrow_mut()
            .storage
            .entry(address)
            .or_default()
            .insert(index, data);
        Ok(data)
    }

    fn block_hash(&self, number: U256) -> Result<B256, Self::Error> {
        let block_number = u64::try_from(number).unwrap();
        match self.data.borrow().block_hashes.get(&block_number) {
            Some(hash) => return Ok(*hash),
            None => {}
        }
        debug!("Fetching block hash {} from rpc", number);
        let block = self
            .tokio_handle
            .block_on(async {
                let block = self.provider.get_block(block_number).await;
                // block?.hash.unwrap()
                block
            })
            .map_err(|err| DbError::GetBlockHash(block_number, eyre::Error::new(err)))?;
        let hash = block.unwrap().hash.unwrap().to_alloy();
        self.data
            .borrow_mut()
            .block_hashes
            .insert(block_number, hash);
        Ok(hash)
    }

    fn code_by_hash(&self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        unreachable!()
    }
}


pub struct ProxyDb<'a, M: Middleware> {
    pub hook_accounts: HashMap<Address, AccountInfo>,
    pub hook_storage: HashMap<Address, HashMap<U256, U256>>,
    pub db: &'a JsonBlockCacheDB<'a, M>,
    pub trace_basic: Vec<Address>,
    pub trace_storage: Vec<(Address, U256)>,
    pub trace_block_hashes: Vec<U256>,
}

impl<'a, M: Middleware + 'static> ProxyDb<'a, M> {
    pub fn new(db: &'a JsonBlockCacheDB<'a, M>) -> Self {
        Self {
            hook_accounts: HashMap::new(),
            hook_storage: HashMap::new(),
            db,
            trace_basic: Vec::new(),
            trace_storage: Vec::new(),
            trace_block_hashes: Vec::new(),
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

    pub fn compact_trace_data(&mut self) -> Result<(MptNode, Map<Address, StorageEntry>, HashSet<Bytes>, Map<u64, B256>)> {
        let cache_data = self.db.data();
        let mut contracts = HashSet::new();

        let mut storage = Map::new();
        let mut state_nodes = HashMap::new();
        let mut state_root_node = MptNode::default();
        let mut dedup_trace_accounts = self.trace_basic.iter().cloned().collect::<HashSet<_>>();
        let dedup_block_numbers = self.trace_block_hashes.iter().cloned().collect::<HashSet<_>>();
        let mut account_storage: HashMap<Address, HashSet<U256>> = HashMap::new();
        for (address, slot) in self.trace_storage.iter() {
            dedup_trace_accounts.insert(address.clone());
            account_storage.entry(address.clone()).or_default().insert(*slot);
        }

        let skip_addrs = vec![DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER];

        for address in dedup_trace_accounts {
            if skip_addrs.contains(&address) {
                continue;
            }
            let info = cache_data.accounts.get(&address).wrap_err("missing account")?;
            let code = info.code.clone().wrap_err("missing code")?;
            if !code.is_empty() {
                contracts.insert(code.bytecode);
            }

            let slots = account_storage.entry(address.clone()).or_default().iter().cloned().collect::<Vec<_>>();
            let proof = self.db.get_proof(address, &slots)?;
   
            let proof_nodes = parse_proof(&proof.account_proof).context("invalid account_proof encoding")?;
            mpt_from_proof(&proof_nodes).context("invalid account_proof")?;
            if let Some(node) = proof_nodes.first() {
                state_root_node = node.clone();
            }
            proof_nodes.into_iter().for_each(|node| {
                state_nodes.insert(node.reference(), node);
            });

            let mut storage_nodes = HashMap::new();
            let mut storage_root_node = MptNode::default();

            let storage_root = proof.storage_hash.to_alloy();
            if proof.storage_proof.is_empty() {
                let storage_root_node = node_from_digest(storage_root);
                storage.insert(address.clone(), (storage_root_node, vec![]));
                continue;
            }

            for storage_proof in proof.storage_proof {
                let proof_nodes = parse_proof(&storage_proof.proof).context("invalid storage_proof encoding")?;
                mpt_from_proof(&proof_nodes).context("invalid storage_proof ")?;

                if let Some(node) = proof_nodes.first() {
                    storage_root_node = node.clone();
                }

                proof_nodes.into_iter().for_each(|node| {
                    storage_nodes.insert(node.reference(), node);
                })
            }
            
            let storage_trie = resolve_nodes(&storage_root_node, &storage_nodes);
            assert_eq!(storage_trie.hash(), storage_root);

            storage.insert(address.clone(), (storage_trie, slots));

        }
        let state_trie = resolve_nodes(&state_root_node, &state_nodes);

        let mut block_hashes = Map::new();

        for (block_number, block_hash) in &cache_data.block_hashes {
            let bn = U256::from_be_bytes(block_number.to_be_bytes());
            if!dedup_block_numbers.contains(&bn) {
                continue;
            }
            block_hashes.insert(block_number.clone(), block_hash.clone());
        }

        Ok((state_trie, storage, contracts, block_hashes))
    }
}

impl<'a, M: Middleware + 'static> Database for ProxyDb<'a, M> {
    type Error = DbError;

    fn basic(&mut self, address:Address) -> Result<Option<AccountInfo> ,Self::Error>  {
        self.trace_basic.push(address);
        let info = match self.hook_accounts.get(&address) {
            Some(info) => {
                self.db.basic(address)?;
                Some(info.clone())
            },
            None => self.db.basic(address)?,
        };
        Ok(info)
    }

    fn code_by_hash(&mut self, _code_hash:B256) -> Result<Bytecode,Self::Error>  {
        todo!()
    }

    fn storage(&mut self, address:Address, index:U256) -> Result<U256,Self::Error>  {
        self.trace_storage.push((address, index));
        let value = match self.hook_storage.get(&address).and_then(|s| s.get(&index)) {
            Some(value) => {
                self.db.storage(address, index)?;
                *value
            },
            None => self.db.storage(address, index)?
        };
        Ok(value)
    }

    fn block_hash(&mut self, number:U256) -> Result<B256, Self::Error>  {
        self.trace_block_hashes.push(number);
        self.db.block_hash(number)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::get_http_provider;
    use alloy_primitives::address;

    #[test]
    fn rpc_cache_db() {
        let provider = get_http_provider("https://rpc.flashbots.net");
        let mut meta = BlockchainDbMeta {
            chain_spec: ChainSpec {
                chain_id: U256::from(1),
                spec_id: SpecId::SHANGHAI,
            },
            header: Default::default(),
        };
        meta.header.number = 18400000u64;
        let db = JsonBlockCacheDB::new(&provider, meta, None);
        let address = address!("c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2");
        let account = db.basic(address).unwrap().unwrap();
        println!("account: {:?}, balance: {}", account, account.balance);
        for i in 0..5 {
            let storage = db.storage(address, U256::from(i)).unwrap();
            println!("storage: {:?}, index: {}", storage, i);
        }
    }
}
