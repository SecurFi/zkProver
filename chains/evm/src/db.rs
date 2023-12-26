use bridge::{BlockHeader, StorageEntry};
use bridge::trie::{MptNode, };
use ethers_providers::Middleware;
use eyre::{Result, Context, ContextCompat};
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
use tracing::{trace, warn};


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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MemDb {
    /// Account related data
    pub accounts: Map<Address, AccountInfo>,
    /// Storage related data
    pub storage: Map<Address, Map<U256, U256>>,
    /// All retrieved block hashes
    pub block_hashes: Map<u64, B256>,
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
                data: Default::default(),
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
        trace!(target : "cache", ?path, "reading json cache");
        let file = fs::File::open(&path).map_err(|err| {
            warn!(?err, ?path, "Failed to read cache file");
            err
        })?;
        let file = std::io::BufReader::new(file);
        let data: JsonBlockCacheData = serde_json::from_reader(file).map_err(|err| {
            warn!(target : "cache", ?err, ?path, "Failed to deserialize cache data");
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
            trace!(target: "cache", "saving json cache path={:?}", path);
            if let Some(parent) = path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::File::create(path)
                .map_err(|e| warn!(target: "cache", "Failed to open json cache for writing: {}", e))
                .and_then(|f| {
                    serde_json::to_writer(BufWriter::new(f), &self.data)
                        .map_err(|e| warn!(target: "cache" ,"Failed to write to json cache: {}", e))
                });
            trace!(target: "cache", "saved json cache path={:?}", path);
        }
    }

    pub fn compact_data(&self) -> Result<(MptNode, Map<Address, StorageEntry>, HashSet<Bytes>, Map<u64, B256>)> {
        let cache_data = self.data.borrow();
        let block_id = Some(cache_data.meta.header.number.into());
        let mut contracts = HashSet::new();
        let mut proofs = Map::new();

        // todo: concurrently fetch all accounts
        for (address, info) in &cache_data.data.accounts {
            let code = info.code.clone().wrap_err("missing code")?;
            if !code.is_empty() {
                contracts.insert(code.bytecode);
            }
            let storage = cache_data.data.storage.get(address);
            let keys = if let Some(storage) = storage {
                storage.keys().cloned().collect::<Vec<_>>()
            } else {
                vec![]
            };
            let resp = self.tokio_handle.block_on(async {
                let locations = keys.into_iter().map(|k| k.to_be_bytes().into()).collect::<Vec<_>>();
                self.provider.get_proof(address.to_ethers(), locations, block_id).await
            })?;
            proofs.insert(address.clone(), resp);
        }
        let mut storage = Map::new();
        let mut state_nodes = HashMap::new();
        let mut state_root_node = MptNode::default();

        for (address, proof) in proofs {
            // println!("storage proof hash: {} {:?}, {}", address, proof.storage_hash, proof.storage_proof.len());

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
            for storage_prof in &proof.storage_proof {
                let proof_nodes = parse_proof(&storage_prof.proof).context("invalid storage_proof encoding")?;
                mpt_from_proof(&proof_nodes).context("invalid storage_proof ")?;

                if let Some(node) = proof_nodes.first() {
                    storage_root_node = node.clone();
                }

                proof_nodes.into_iter().for_each(|node| {
                    storage_nodes.insert(node.reference(), node);
                })
            }
            let storage_root = proof.storage_hash.to_alloy();
            if proof.storage_proof.is_empty() {
                let storage_root_node = node_from_digest(storage_root);
                storage.insert(address, (storage_root_node, vec![]));
                continue;
            }
            let storage_trie = resolve_nodes(&storage_root_node, &storage_nodes);
            assert_eq!(storage_trie.hash(), storage_root);

            let slots = proof
                .storage_proof
                .iter()
                .map(|p| U256::from_be_bytes(p.key.into()))
                .collect();
            storage.insert(address, (storage_trie, slots));

        }
        let state_trie = resolve_nodes(&state_root_node, &state_nodes);

        let mut block_hashes = Map::new();
        for (block_number, block_hash) in &cache_data.data.block_hashes {
            block_hashes.insert(block_number.clone(), block_hash.clone());
        }

        Ok((state_trie, storage, contracts, block_hashes))
    }
}

impl<'a, M: Middleware> DatabaseRef for JsonBlockCacheDB<'a, M>
where
    M::Error: 'static,
{
    type Error = DbError;
    fn basic(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        match self.data.borrow().data.accounts.get(&address) {
            Some(account) => return Ok(Some(account.clone())),
            None => {}
        }
        println!("Fetching account {} from rpc", address);
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
            .data
            .accounts
            .insert(address, account_info.clone());
        Ok(Some(account_info))
    }

    fn storage(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        let value = self
            .data
            .borrow()
            .data
            .storage
            .get(&address)
            .and_then(|s| s.get(&index).copied());
        if let Some(value) = value {
            return Ok(value);
        }
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
            .data
            .storage
            .entry(address)
            .or_default()
            .insert(index, data);
        Ok(data)
    }

    fn block_hash(&self, number: U256) -> Result<B256, Self::Error> {
        let block_number = u64::try_from(number).unwrap();
        match self.data.borrow().data.block_hashes.get(&block_number) {
            Some(hash) => return Ok(*hash),
            None => {}
        }
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
            .data
            .block_hashes
            .insert(block_number, hash);
        Ok(hash)
    }

    fn code_by_hash(&self, _code_hash: B256) -> Result<Bytecode, Self::Error> {
        unreachable!()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonBlockCacheData {
    pub meta: BlockchainDbMeta,
    pub data: MemDb,
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
