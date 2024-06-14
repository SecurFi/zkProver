use clap::Parser;
use clio::OutputPath;
use anyhow::Result;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::BlockId;
use alloy_primitives::U256;
use chains_evm_core::{
    block::BlockHeader, db::{BlockchainDbMeta, ChainSpec, JsonBlockCacheDB}, 
    deal::DealRecord, poc_compiler::compile_poc, preflight::build_input
};
use risc0_zkvm::{ExecutorEnv, ExecutorImpl};
use guests::{EXPLOIT_ID, EXPLOIT_ELF};
use std::time::Instant;

use crate::proof::Proof;

#[derive(Parser, Debug)]
pub struct EvmArgs {
    /// The poc contract
    poc: String,

    #[clap(short, long)]
    rpc_url: String,

    #[clap(short, long)]
    block_number: Option<u64>,
    /// Set the token balances of the poc contract.
    /// Examples: 1ether, 0xdac17f958d2ee523a2206206994597c13d831ec7:10gwei
    #[clap(short, long)]
    deal: Option<Vec<DealRecord>>,
    /// Just simulate the exploit tx, don't actually generate a proof.
    #[clap(long)]
    pub dry_run: bool,

    /// Output file
    #[clap(long, short, value_parser, default_value = "proof.bin")]
    output: OutputPath,
}

impl EvmArgs {
    /// Executes the `evm` subcommand.
    pub async fn run(self) -> Result<()> {
        let contract = compile_poc(self.poc)?;
        let poc_code_hash = contract.hash_slow();

        let provider = ProviderBuilder::new()
            .on_http(self.rpc_url.as_str().try_into()?)?;

        let block_id = match self.block_number {
            Some(n) => BlockId::number(n),
            None => BlockId::safe()
        };
        let chain_id = provider.get_chain_id().await?;
        let block = provider.get_block(block_id, false).await?.expect("could not found block");
        let block_number = block.header.number.unwrap();
        println!("Chain: {:?}", chain_id);
        println!("Block Number: {:?}", block_number);
        println!("Poc Code Hash: {:?}", poc_code_hash);
        let rpc_cache_dir = dirs_next::home_dir().expect("home dir not found").join(".securfi").join("cache").join("rpc");
        let cache_path =  rpc_cache_dir.join(format!("{}", chain_id)).join(format!("{}.json", block.header.number.unwrap()));

        let header: BlockHeader = block.header.try_into()?;

        let chain_spec = ChainSpec::mainnet();
        let meta = BlockchainDbMeta {
            chain_spec: chain_spec.clone(), // currently only supports mainnet and shanghai
            header: header.clone(),
        };
        let db = JsonBlockCacheDB::new(&provider, meta, Some(cache_path));

        // todo: add deal
        let initial_balance = U256::ZERO;
        let exploit_input = build_input(contract, header, &db, initial_balance)?;

        let zk_env = ExecutorEnv::builder()
            .write(&exploit_input)?
            .build()?;
        
        let mut exec = ExecutorImpl::from_elf(zk_env, EXPLOIT_ELF)?;
        let session = exec.run()?;
        let evm_id: Vec<u8> = EXPLOIT_ID.iter().flat_map(|x| x.to_le_bytes()).collect();

        
        if !self.dry_run {
            println!(
                "starting generate zk proof, image id: {}",
                hex::encode(evm_id)
            );
            let start = Instant::now();
            let receipt = session.prove()?.receipt;
            let _ = receipt.verify(EXPLOIT_ID);
            let duration = start.elapsed();

            let spec_name: &'static str = chain_spec.spec_id.into();
            let image_id = hex::encode(EXPLOIT_ID.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>());
            let proof = Proof {
                version: env!("CARGO_PKG_VERSION").to_string(),
                image_id: image_id,
                chain_id: chain_id,
                spec_id: spec_name.to_string(),
                block_number: block_number,
                poc_code_hash: poc_code_hash,
                deals: self.deal.unwrap_or_default(),
                receipt: Some(receipt),
            };
            let output = self.output.create()?;
            proof.save(output)?;
            println!("generate zk proof success, time: {:?}", duration);
        }
        Ok(())
    }
}

