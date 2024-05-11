use alloy_primitives::{Address, U256, BlockHash, BlockNumber, B256, B64, Bloom, Bytes};
use alloy_rpc_types::Header;
use revm::primitives::BlockEnv;
use anyhow::{Context, Result};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
    pub gas_limit: u64,
    /// Gas Used
    pub gas_used: u64,
    /// Timestamp
    pub timestamp: u64,
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
    /// Blob gas used (if past Cancun)
    pub blob_gas_used: Option<u64>,
    /// Excess blob gas (if past Cancun)
    pub excess_blob_gas: Option<u64>,
    /// Parent beacon block root (if past Cancun)
    pub parent_beacon_block_root: Option<B256>,
}

impl BlockHeader {
    pub fn into_block_env(&self) -> BlockEnv {
        let mut block_env = BlockEnv::default();
        block_env.number = U256::from(self.number);
        block_env.timestamp = U256::from(self.timestamp);
        block_env.coinbase = self.author;
        block_env.difficulty = self.difficulty;
        block_env.gas_limit = U256::from(self.gas_limit);
        // block_env.basefee = self.base_fee_per_gas;
        block_env.prevrandao = Some(self.mix_hash);
        if let Some(excess_blob_gas) = self.excess_blob_gas {
            block_env.set_blob_excess_gas_and_price(excess_blob_gas);
        }
        return block_env;
    }
}

impl TryFrom<Header> for BlockHeader {
    type Error = anyhow::Error;

    fn try_from(header: Header) -> Result<Self, Self::Error> {
        Ok(Self { 
            parent_hash: header.parent_hash, 
            uncles_hash: header.uncles_hash, 
            author: header.miner, 
            state_root: header.state_root, 
            transactions_root: header.transactions_root, 
            receipts_root: header.receipts_root, 
            logs_bloom: header.logs_bloom, 
            difficulty: header.difficulty, 
            number: header.number.context("block number is missing")?, 
            gas_limit: header.gas_limit.try_into()?, 
            gas_used: header.gas_used.try_into()?, 
            timestamp: header.timestamp, 
            extra_data: header.extra_data, 
            mix_hash: header.mix_hash.context("mix_hash is missing")?, 
            nonce: header.nonce.context("nonce is missing")?, 
            base_fee_per_gas: header.base_fee_per_gas.context("base_fee_per_gas is missing")?.try_into()?, 
            withdrawals_root: header.withdrawals_root, 
            blob_gas_used: header.blob_gas_used.map(|x| x.try_into().unwrap()), 
            excess_blob_gas: header.excess_blob_gas.map(|x| x.try_into().unwrap()), 
            parent_beacon_block_root: header.parent_beacon_block_root
        })
    }
}

