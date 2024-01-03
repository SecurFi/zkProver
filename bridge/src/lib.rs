use alloy_primitives::{Address, address, BlockHash, BlockNumber, Bloom, Bytes, bytes, B256, B64, U256, b256};
use alloy_rlp_derive::RlpEncodable;
use keccak::KECCAK_EMPTY;
use revm::{EVM, primitives::{AccountInfo, Bytecode, SpecId, TxEnv, TransactTo, BlockEnv}, db::DatabaseRef};
use trie::{MptNode, StateAccount, EMPTY_ROOT};
use core::{mem, panic};
use std::collections::BTreeMap as Map;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256,};
use sha2::Sha256;
pub mod trie;
pub mod keccak;

#[cfg(not(target_os = "zkvm"))]
use alloy_rlp::Buf;

pub struct DbAccount {
    pub info: AccountInfo,
    pub storage: Map<U256, U256>,
}

pub struct Db {
    pub accounts: Map<Address, DbAccount>,
    pub block_hashes: Vec<(u64, B256)>,
}

impl DatabaseRef for Db {
    type Error = ();

    fn basic(&self, address:Address) -> Result<Option<AccountInfo> ,Self::Error>  {
        match self.accounts.get(&address) {
            Some(db_account) => Ok(Some(db_account.info.clone())),
            None => {
                // println!("address not found {}", address);
                Err(())
            }
        }
    }

    fn code_by_hash(&self, _code_hash:B256) -> Result<Bytecode,Self::Error>  {
        panic!()
    }

    fn storage(&self, address:Address, index:U256) -> Result<U256,Self::Error>  {
        match self.accounts.get(&address) {
            Some(db_account) => match db_account.storage.get(&index) {
                Some(value) => Ok(*value),
                None => {
                    if address == DEFAULT_CONTRACT_ADDRESS {
                        Ok(U256::from(0))
                    } else {
                        // println!("storage not found {} {}", address, index);
                        Err(())
                    }
                }
            },
            None => {
                Err(())
            }
        }
    }

    fn block_hash(&self, number:U256) -> Result<B256,Self::Error>  {
        let block_no: u64 = number.try_into().unwrap();
        let entry = self.block_hashes.iter()
        .find(|(_block_no, _)| *_block_no == block_no);
        match entry {
            Some((_block_no, hash)) => Ok(*hash),
            None => {
                // println!("block hash not found {}", number);
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
pub const DEFAULT_CALL_DATA: Bytes = bytes!("63d9b770");

pub const EMPTY_LIST_HASH: B256 =
    b256!("1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347");


#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, RlpEncodable)]
#[rlp(trailing)]
pub struct BlockHeader {
    /// Hash of the parent
    pub parent_hash: BlockHash,
    /// Hash of the uncles
    pub uncles_hash: B256,
    /// Miner/author's address.
    pub author: Address,
    /// State root hash
    pub state_root: B256,
    /// Transactions root hash
    pub transactions_root: B256,
    /// Transactions receipts root hash
    pub receipts_root: B256,
    /// Logs bloom
    pub logs_bloom: Bloom,
    /// Difficulty
    pub difficulty: U256,
    /// Block number. None if pending.
    pub number: BlockNumber,
    /// Gas Limit
    pub gas_limit: U256,
    /// Gas Used
    pub gas_used: U256,
    /// Timestamp
    pub timestamp: U256,
    /// Extra data
    pub extra_data: Bytes,
    /// Mix Hash
    pub mix_hash: B256,
    /// Nonce
    pub nonce: B64,
    /// Base fee per unit of gas (if past London)
    pub base_fee_per_gas: U256,
    /// Withdrawals root hash (if past Shanghai)
    pub withdrawals_root: Option<B256>,
}

impl Default for BlockHeader {
    fn default() -> Self {
        BlockHeader {
            parent_hash: B256::ZERO,
            uncles_hash: EMPTY_LIST_HASH,
            author: Address::ZERO,
            state_root: EMPTY_ROOT,
            transactions_root: EMPTY_ROOT,
            receipts_root: EMPTY_ROOT,
            logs_bloom: Bloom::default(),
            difficulty: U256::ZERO,
            number: 0,
            gas_limit: U256::ZERO,
            gas_used: U256::ZERO,
            timestamp: U256::ZERO,
            extra_data: Bytes::new(),
            mix_hash: B256::ZERO,
            nonce: B64::ZERO,
            base_fee_per_gas: U256::ZERO,
            withdrawals_root: None,
        }
    }
}

impl BlockHeader {
    pub fn hash(&self) -> BlockHash {
        keccak(alloy_rlp::encode(self)).into()
    }
}

pub type StorageEntry = (MptNode, Vec<U256>);

#[derive(Debug, Deserialize, Serialize)]
pub struct Artifacts {
    pub storage: Map<Address, Map<U256, U256>>, // the storage patch of the current block
    pub initial_balance: U256,     // the initial balance of the poc contract
}

impl Artifacts {
    pub fn hash(&self) -> B256 {
        let mut hasher = Sha256::new();
        for (address, storage) in self.storage.iter() {
            hasher.update(address.0);
            for (key, value) in storage.iter() {
                hasher.update(key.to_be_bytes::<32>());
                hasher.update(value.to_be_bytes::<32>());
            }
        }
        if self.initial_balance!= U256::ZERO {
            hasher.update(self.initial_balance.to_be_bytes::<32>());
        }
        
        B256::from_slice(hasher.finalize().as_slice())
    }
}


#[derive(Debug, Deserialize, Serialize)]
pub struct VmInput {
    pub header: BlockHeader,
    pub state_trie: MptNode,
    pub storage_trie: Map<Address, StorageEntry>,
    pub contracts: Vec<Bytes>,
    pub block_hashes: Vec<(u64, BlockHash)>,
    pub poc_contract: Bytes,
    pub artifacts: Artifacts,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diff<T> {
    pub old: T,
    pub new: T,
}

#[derive(Debug, Clone)]
pub struct StateDiff {
    balance: Option<Diff<U256>>,
    storage: Map<U256, Diff<U256>>,
}


#[derive(Debug, Clone)]
pub struct VmOutput {
    pub artifacts_hash: B256,
    pub block_hashes: Vec<(u64, BlockHash)>, // the biggest block number is the base block's number
    pub poc_contract_hash: B256,
    pub state_diff: Map<Address, StateDiff>,
}


impl VmOutput {
    pub fn encode(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.artifacts_hash.as_slice());
        buf.push(*self.block_hashes.len().to_be_bytes().last().unwrap());
        for (block_number, block_hash) in self.block_hashes.iter() {
            buf.extend_from_slice(&block_number.to_be_bytes());
            buf.extend_from_slice(&block_hash.0);
        }
        
        buf.extend_from_slice(self.poc_contract_hash.as_slice());

        buf.push(*self.state_diff.len().to_be_bytes().last().unwrap());
        for (address, state_diff) in self.state_diff.iter() {
            buf.extend_from_slice(address.as_slice());
            // buf.push(state_diff.balance.is_some() as u8);
            match &state_diff.balance {
                Some(balance) => {
                    buf.push(1);
                    buf.extend_from_slice(&balance.old.to_be_bytes::<32>());
                    buf.extend_from_slice(&balance.new.to_be_bytes::<32>());
                },
                None => buf.push(0),
            };

            buf.push(*state_diff.storage.len().to_be_bytes().last().unwrap());
            for (key, value) in state_diff.storage.iter() {
                buf.extend_from_slice(&key.to_be_bytes::<32>());
                buf.extend_from_slice(&value.old.to_be_bytes::<32>());
                buf.extend_from_slice(&value.new.to_be_bytes::<32>());
            }
        }
        buf
    }

    #[cfg(not(target_os = "zkvm"))]
    pub fn decode<'a>(buf: &mut &[u8]) -> Self {
        let artifacts_hash = B256::from_slice(unsafe { buf.get_unchecked(..32) });
        buf.advance(32);
        let block_hashes_len = buf.get_u8();
        let mut block_hashes = Vec::new();
        for _ in 0..block_hashes_len {
            let block_number = buf.get_u64();
            let block_hash = B256::from_slice(unsafe { buf.get_unchecked(..32) });
            buf.advance(32);
            block_hashes.push((block_number, block_hash));
        }
        let poc_contract_hash = B256::from_slice(unsafe { buf.get_unchecked(..32) });
        buf.advance(32);
        let state_diff_len = buf.get_u8();
        let mut state_diff = Map::new();
        for _ in 0..state_diff_len {
            let address = Address::from_slice(unsafe { buf.get_unchecked(..20) });
            buf.advance(20);
            let balance_diff = if buf.get_u8() == 1 {
                let old = U256::try_from_be_slice(unsafe { buf.get_unchecked(..32) }).unwrap();
                buf.advance(32);
                let new = U256::try_from_be_slice(unsafe { buf.get_unchecked(..32) }).unwrap();
                buf.advance(32);
                Some(Diff { old, new })
            } else {
                None
            };
            let storage_diff_len =  buf.get_u8();
            let mut storage_diff = Map::new();
            for _ in 0..storage_diff_len {
                let key = U256::try_from_be_slice(unsafe { buf.get_unchecked(..32) }).unwrap();
                buf.advance(32);
                let old = U256::try_from_be_slice(unsafe { buf.get_unchecked(..32) }).unwrap();
                buf.advance(32);
                let new = U256::try_from_be_slice(unsafe { buf.get_unchecked(..32) }).unwrap();
                buf.advance(32);
                storage_diff.insert(key, Diff { old, new });
            }
            state_diff.insert(address, StateDiff { balance: balance_diff, storage: storage_diff });
        }
        VmOutput { artifacts_hash, block_hashes, poc_contract_hash, state_diff }
    }

}



#[inline]
pub fn keccak(data: impl AsRef<[u8]>) -> [u8; 32] {
    Keccak256::digest(data).into()
}

pub fn guest_mem_forget<T>(_t: T) {
    #[cfg(target_os = "zkvm")]
    core::mem::forget(_t)
}

#[allow(non_snake_case)]
pub fn get_specId_from_block_number(_block_number: u64) -> SpecId {
    return SpecId::SHANGHAI;
    // match block_number {
    //     _ if block_number >= 17034870 => SpecId::SHANGHAI,
    //     _ if block_number >= 15537394 => SpecId::MERGE,
    //     _ => SpecId::FRONTIER,
    // }
}

pub fn execute_vm(mut input: VmInput) -> VmOutput {
    if input.header.state_root != input.state_trie.hash() {
        panic!();
    }
    let contracts: Map<B256, Bytes> = mem::take(&mut input.contracts).into_iter().map(|bytes| (keccak(&bytes).into(), bytes)).collect();
    let mut accounts = Map::new();
    for (address, (storage_trie, slots)) in &mut input.storage_trie {
        let slots = mem::take(slots);
        let address_trie_key = keccak(address);
        let state_account = input.state_trie.get_rlp::<StateAccount>(&address_trie_key).unwrap().unwrap_or_default();
        if storage_trie.hash() != state_account.storage_root {
            panic!();
        }
        let code_hash = state_account.code_hash;
        let bytecode = if code_hash.0 == KECCAK_EMPTY.0 {
            Bytecode::new()
        } else {
            let bytes = contracts.get(&code_hash).unwrap().clone();
            Bytecode::new_raw(bytes)
        };

        let mut storage = Map::new();
        for slot in slots {
            let trie_key = &keccak(slot.to_be_bytes::<32>());
            let value: U256 = storage_trie.get_rlp(trie_key).unwrap().unwrap_or_default();
            storage.insert(slot, value);
        }
        let db_account = DbAccount {
            info: AccountInfo {
                balance: state_account.balance,
                nonce: state_account.nonce,
                code_hash: state_account.code_hash,
                code: Some(bytecode),
            },
            storage: storage,
        };
        accounts.insert(*address, db_account);
    }
    // guest_mem_forget(contracts);

    let artifacts_hash = input.artifacts.hash();

    accounts.insert(DEFAULT_CALLER, DbAccount {
        info: AccountInfo {
            balance: U256::ZERO,
            nonce: 1,
            code_hash: KECCAK_EMPTY,
            code: None,
        },
        storage: Map::new(),
    });

    let poc_contract = Bytecode::new_raw(input.poc_contract);
    let poc_contract_hash = poc_contract.hash_slow();
    accounts.insert(DEFAULT_CONTRACT_ADDRESS, DbAccount {
        info: AccountInfo {
            balance: input.artifacts.initial_balance,
            nonce: 1,
            code_hash: poc_contract_hash,
            code: Some(poc_contract),
        },
        storage: Map::new(),
    });

    for (address, storage) in input.artifacts.storage {
        accounts.get_mut(&address).unwrap().storage.extend(
            storage.into_iter().map(|(key, value)|(key, value))
        )
    }

    let mut block_hashes = vec![(input.header.number, input.header.hash())];
    for (block_number, hash) in input.block_hashes.iter() {
        if *block_number >= input.header.number {
            panic!()
        }
        block_hashes.push((*block_number, *hash));
    }

    let db = Db {
        accounts: accounts,
        block_hashes: input.block_hashes,
    };


    let mut evm = EVM::new();
    evm.database(db);
    evm.env.tx = TxEnv {
        caller: DEFAULT_CALLER,
        transact_to: TransactTo::Call(DEFAULT_CONTRACT_ADDRESS),
        data: DEFAULT_CALL_DATA,
        value: U256::ZERO,
        ..Default::default()
    };
    evm.env.block = BlockEnv {
        number: U256::from(input.header.number),
        timestamp: U256::from(input.header.timestamp),
        ..Default::default()
    };
    evm.env.cfg.spec_id = get_specId_from_block_number(input.header.number);
    
    let result_and_state = evm.transact_ref().unwrap();
    if !result_and_state.result.is_success() {
        panic!()
    }

    let old_db = evm.take_db();
    let mut state_diff = Map::new();

    for (address, account) in result_and_state.state {
        let old_balance = old_db.accounts.get(&address).map(|a|a.info.balance).unwrap_or(U256::ZERO);
        let new_balance = if account.is_selfdestructed() {
            U256::ZERO
        } else {
            account.info.balance

        };
        let balance_state = if old_balance != new_balance {
            Some(Diff{
                old: old_balance,
                new: account.info.balance,
            })
        } else {
            None
        };
        let mut storage_state = Map::new();
        for (key, sslot) in account.storage {
            if !sslot.is_changed() {
                continue;
            }
            let old_value = old_db.accounts.get(&address).map(|x| x.storage.get(&key).unwrap_or(&U256::ZERO).clone()).unwrap_or(U256::ZERO);
            let new_value = sslot.present_value;
            if new_balance!= old_value {
                storage_state.insert(key, Diff { old: old_value, new: new_value });
            }
        }
        if balance_state.is_none() && storage_state.is_empty() {
            continue;
        }
        state_diff.insert(address, StateDiff{
            balance: balance_state,
            storage: storage_state,
        });
    }
    
    VmOutput {
        artifacts_hash,
        block_hashes,
        poc_contract_hash,
        state_diff: state_diff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shanghai() {
        let value = json!({
            "parent_hash": "0xc2558f8143d5f5acb8382b8cb2b8e2f1a10c8bdfeededad850eaca048ed85d8f",
            "uncles_hash": "0x1dcc4de8dec75d7aab85b567b6ccd41ad312451b948a7413f0a142fd40d49347",
            "author": "0x388c818ca8b9251b393131c08a736a67ccb19297",
            "state_root": "0x7fd42f5027bc18315b3781e65f19e4c8828fd5c5fce33410f0fb4fea0b65541f",
            "transactions_root": "0x6f235d618461c08943aa5c23cc751310d6177ab8a9b9a7b66ffa637d988680e6",
            "receipts_root": "0xe0ac34bafdd757bcca2dea27a3fc5870dd0836998877e29361c1fc55e19416ec",
            "logs_bloom": "0xb06769bc11f4d7a51a3bc4bed59367b75c32d1bd79e5970e73732ac0eed0251af0e2abc8811fc1b4c5d45a4a4eb5c5af9e73cc9a8be6ace72faadc03536d6b69fcdf80116fd89f7efbdbf38ff957e8f6ae83ccac60cf4b7c8b1c9487bebfa8ed6e42297e17172d5b678dd3f283b22f49bbf4a0565eb93d9d797b2f9a0adaff9813af53d6fffa71d5a6fb056ab73ca87659dc97c19f99839c6c3138e527161b4dfee8b1f64d42f927abc745f3ff168e8e9510e2e079f4868ba8ff94faf37c9a7947a43c1b4c931dfbef88edeb2d7ede5ceaebc85095cfbbd206646def0138683b687fa63fdf22898260d616bc714d698bc5748c7a5bff0a4a32dd797596a794a0",
            "difficulty": "0x0",
            "number": 17034870,
            "gas_limit": "0x1c9c380",
            "gas_used": "0x1c9bfe2",
            "timestamp": "0x6437306f",
            "extra_data": "0xd883010b05846765746888676f312e32302e32856c696e7578",
            "mix_hash": "0x812ed704cc408c435c7baa6e86296c1ac654a139ae8c4a26d6460742b951d4f9",
            "nonce": "0x0000000000000000",
            "base_fee_per_gas": "0x42fbae6d5",
            "withdrawals_root": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"
        });
        let header: BlockHeader = serde_json::from_value(value).unwrap();
        // verify that bincode serialization works
        let _: BlockHeader = bincode::deserialize(&bincode::serialize(&header).unwrap()).unwrap();

        assert_eq!(
            "0xe22c56f211f03baadcc91e4eb9a24344e6848c5df4473988f893b58223f5216c",
            header.hash().to_string()
        )
    }
}
