use clap::Parser;
use clio::{Input, OutputPath};
use anyhow::Result;
use std::io::Write;
use alloy_provider::{Provider, ProviderBuilder};
use alloy_rpc_types::BlockId;
use alloy_primitives::U256;
use chains_evm_core::{
    block::BlockHeader, db::{BlockchainDbMeta, ChainSpec, JsonBlockCacheDB}, deal::DealRecord, poc_compiler::compile_poc, preflight::build_input
};
use risc0_zkvm::{serde::to_vec, Receipt};
use crate::proof::Proof;
use guests::EXPLOIT_ID;


#[derive(Parser, Debug)]
pub struct PreArgs {
    poc: String,

    #[clap(short, long)]
    rpc_url: String,

    #[clap(short, long)]
    block_number: Option<u64>,
    /// Set the balances of the exploit contract.
    /// Examples: 1ether, 0xdac17f958d2ee523a2206206994597c13d831ec7:10gwei
    #[clap(short, long)]
    deal: Option<Vec<DealRecord>>,

    /// limit the max gas used
    #[clap(short, long)]
    gas: Option<u64>,

    /// Output file
    #[clap(long, short, value_parser, default_value = "input.hex")]
    output: OutputPath,

    #[clap(long, short, value_parser, default_value = "sketch_proof.bin")]
    proof: OutputPath,
}

#[derive(Parser, Debug)]
pub struct PackArgs {
    #[clap(long, short, value_parser, default_value = "input.hex")]
    receipt: Input,

    #[clap(long, value_parser, default_value = "sketch_proof.bin")]
    proof: Input,

    #[clap(long, value_parser, default_value = "proof.bin")]
    output: OutputPath,
}

impl PreArgs {
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


        let mut v8bytes: Vec<u8> = Vec::new();
        v8bytes.extend_from_slice(bytemuck::cast_slice(&to_vec(&exploit_input).unwrap()));
        let mut output = self.output.create()?;
        output.write_all(&v8bytes)?;

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
            receipt: None,
        };
        let output = self.proof.create()?;
        proof.save(output)?;
        
        return Ok(());
    }
}

impl PackArgs {
    pub fn run(self) -> Result<()> {
        let mut proof = Proof::load(self.proof)?;
        let receipt: Receipt = bincode::deserialize_from(self.receipt)?;
        proof.receipt = Some(receipt);
        let output = self.output.create()?;
        proof.save(output)?;
        return Ok(());
    }
}
