use clap::Parser;
use clio::{Input, Output};
use anyhow::{Result, bail};
use hex::FromHex;
use revm_primitives::db::DatabaseRef;
use serde::{Deserialize, Serialize};
use alloy_rpc_types::BlockId;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_primitives::{B256, U256, Address};
use bridge::{DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER};
use chains_evm_core::{
    balance_change::{compute_asset_change, AssetChange},
    block::BlockHeader,
    db::{BlockchainDbMeta, ChainSpec, JsonBlockCacheDB},
    deal::DealRecord,
    state_diff::{compute_state_diff, StateDiff}
};
use risc0_zkvm::sha::Digest;
use bridge::ExploitOutput;
use crate::proof::Proof;


#[derive(Parser, Debug)]
pub struct VerifyArgs {
    /// proof file
    path: Input,

    /// Output file
    #[clap(long, short, value_parser, default_value = "-")]
    output: Output,

    #[clap(short, long)]
    rpc_url: String,
}


#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VerifyResult {
    pub version: String,
    pub image_id: String,
    pub chain_id: u64,
    pub spec_id: String,
    pub block_number: u64,
    pub poc_code_hash: B256,
    pub deals: Vec<DealRecord>,
    pub state_diff: StateDiff,
    pub asset_change: Vec<AssetChange>,
    pub gas_used: u64,
}


async fn verify(proof: Proof, rpc_url: String) -> Result<VerifyResult> {
    let image_id = Digest::from_hex(proof.image_id.clone())?;
    proof.receipt.clone().unwrap().verify(image_id)?;

    let output: ExploitOutput = proof.receipt.unwrap().journal.decode()?;
    let block_id = BlockId::number(proof.block_number);
    let provider = ProviderBuilder::new()
            .on_http(rpc_url.as_str().try_into()?)?;

    let block = provider.get_block(block_id, false).await?.expect("could not found block");
    let header: BlockHeader = block.header.try_into()?;

    if output.input.block_env != header.into_block_env() {
        bail!("block env mismatch")
    }
    
    // verify db
    let rpc_cache_dir = dirs_next::home_dir().expect("home dir not found").join(".securfi").join("cache").join("rpc");
    let cache_path =  rpc_cache_dir.join(format!("{}", proof.chain_id)).join(format!("{}.json", proof.block_number));
    let chain_spec = ChainSpec::mainnet();
    let meta = BlockchainDbMeta {
        chain_spec: chain_spec.clone(), // currently only supports mainnet and shanghai
        header: header,
    };
    let rpc_db = JsonBlockCacheDB::new(&provider, meta, Some(cache_path));
    let initial_balance = U256::ZERO;

    for (address, acc_storage) in output.input.db.accounts.iter() {
        let address = address.clone();
        if address == DEFAULT_CONTRACT_ADDRESS {
            if acc_storage.info.balance != initial_balance {
                bail!("balance is not correct")
            }
            if acc_storage.info.code_hash != proof.poc_code_hash {
                bail!("code hash is not correct")
            }
            continue;
        }
        if address == DEFAULT_CALLER {
            if acc_storage.info.balance != initial_balance {
                bail!("balance is not correct")
            }
            continue;
        }
        let info = rpc_db.basic_ref(address)?.unwrap();
        if info != acc_storage.info {
            bail!("account info is not correct")
        }
        for (key, value) in acc_storage.storage.iter() {
            let slot = rpc_db.storage_ref(address, *key)?;
            if slot != *value {
                bail!("storage slot is not correct")
            }
        }
    }

    for (block_number, block_hash) in output.input.db.block_hashes.iter() {
        if *block_hash != rpc_db.block_hash_ref(U256::from(*block_number))? {
            bail!("block hash is not correct")
        }
    }

    let state_diff = compute_state_diff(&output.state, &output.input.db);

    let accounts: Vec<Address> = output.input.db.accounts.keys().cloned().collect();

    let asset_change = compute_asset_change(&accounts, &output.input.db, output.state)?;

    Ok(VerifyResult {
        version: proof.version,
        image_id: proof.image_id,
        chain_id: proof.chain_id,
        spec_id: proof.spec_id,
        block_number: proof.block_number,
        poc_code_hash: proof.poc_code_hash,
        deals: proof.deals,
        gas_used: output.gas_used,
        state_diff: state_diff,
        asset_change: asset_change,
    })
}


impl VerifyArgs {
    pub async fn run(self) -> Result<()> {
        let proof = Proof::load(self.path)?;
        let result = verify(proof, self.rpc_url).await?;

        serde_json::to_writer(self.output, &result)?;
        Ok(())
    }
}