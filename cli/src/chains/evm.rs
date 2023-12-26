use std::io::Write;
use tempfile::tempdir;
use risc0_zkvm::{ExecutorEnv, serde::to_vec, ExecutorImpl, FileSegmentRef};
use ethers_core::types::BlockNumber;
use ethers_providers::Middleware;
use clap::Parser;
use clio::Output;
use eyre::{Result, bail, ContextCompat};
use tokio::time::Instant;
use crate::proof::Proof;
use bridge::{BlockHeader, DEFAULT_CONTRACT_ADDRESS, VmInput, Artifacts, VmOutput};
use chains_evm::{
    poc_compiler::compile_poc,
    deal::{DealRecord, deal, StoragePatch}, 
    provider::try_get_http_provider,
    utils::parse_ether_value,
    db::{BlockchainDbMeta, ChainSpec, JsonBlockCacheDB},
    evm_primitives::{U256, ToAlloy, Bloom}, sim::sim_poc_tx,
};


#[cfg(feature = "prover")]
use zk_guests::{EVM_ELF, EVM_ID};


#[derive(Parser, Debug)] // requires `derive` feature
pub struct EvmArgs {
    /// The Exploit contract
    contract: String,

    #[clap(short, long)]
    rpc_url: String,

    #[clap(short, long)]
    block_number: Option<u64>,
    /// Set the balances of the exploit contract.
    /// Examples: 1ether, 0xdac17f958d2ee523a2206206994597c13d831ec7:10gwei
    #[clap(short, long)]
    deal: Option<Vec<DealRecord>>,
    /// Just simulate the exploit tx, don't actually generate a proof.
    #[clap(long)]
    pub dry_run: bool,
    /// Dump the vm input to a file.
    #[clap(long)]
    dump_input: Option<Output>,

    #[clap(long, value_parser = parse_ether_value)]
    initial_balance: Option<U256>,
    /// Output file 
    #[clap(long, short, value_parser, default_value = "proof.bin")]
    output: Output,
}

impl EvmArgs {
    /// Executes the `evm` subcommand.
    pub async fn run(mut self) -> Result<()> {
        let poc_runtime_bytecode = compile_poc(self.contract)?;
        let provider = try_get_http_provider(self.rpc_url)?;
        let block_id = match self.block_number {
            Some(n) => BlockNumber::from(n),
            None => BlockNumber::Safe,
        };
        let block = provider.get_block(block_id).await?;
        
        let block = block.expect("cound not found block");
        println!("base block number: {}", block.number.unwrap());

        let header = BlockHeader {
            parent_hash: block.parent_hash.to_alloy(),
            uncles_hash: block.uncles_hash.to_alloy(),
            author: block.author.context("author missing")?.to_alloy(),
            state_root: block.state_root.to_alloy(),
            transactions_root: block.transactions_root.to_alloy(),
            receipts_root: block.receipts_root.to_alloy(),
            logs_bloom: Bloom::from_slice(
                block.logs_bloom.context("logs_bloom missing")?.as_bytes()
            ),
            difficulty: block.difficulty.to_alloy(),
            number: block.number.context("block number missing")?.as_u64(),
            gas_limit: block.gas_limit.to_alloy(),
            gas_used: block.gas_used.to_alloy(),
            timestamp: block.timestamp.to_alloy(),
            extra_data: block.extra_data.to_alloy(),
            mix_hash: block.mix_hash.context("mix_hash missing")?.to_alloy(),
            nonce: block.nonce.context("nonce missing")?.0.into(),
            base_fee_per_gas: block.base_fee_per_gas.context("base_fee_per_gas missing")?.to_alloy(),
            withdrawals_root: block.withdrawals_root.map(|x| x.to_alloy()),
        };
        
        if header.hash() != block.hash.unwrap().to_alloy() {
            bail!("block header build failed")
        }

        println!("block hash: {}", block.hash.unwrap());
        println!("header: {:#?}", header);
        println!("EVM SIZE: {}", EVM_ELF.len());
        let meta = BlockchainDbMeta {
            chain_spec: ChainSpec::mainnet(),
            header: header.clone(),
        };

        let rpc_cache_dir = dirs_next::home_dir().expect("home dir not found").join(".0xhacked").join("cache").join("rpc");
        let cache_path =  rpc_cache_dir.join(format!("{}", meta.chain_spec.chain_id)).join(format!("{}.json", header.number));
        let db = JsonBlockCacheDB::new(&provider, meta, Some(cache_path));

        // deal(db, rows)
        let deal_records: Vec<DealRecord> = self
            .deal
            .unwrap_or_default()
            .iter()
            .map(|x| DealRecord {
                address: DEFAULT_CONTRACT_ADDRESS,
                ..x.clone()
            })
            .collect();
        
        let mut storage_patch: StoragePatch = StoragePatch::new();
        if deal_records.len() > 0 {
            storage_patch = deal(&db, &deal_records)?;
        }
        println!("deal state: {:#?}", storage_patch);

        let initial_balance = self.initial_balance.unwrap_or(U256::ZERO);
        // pre run
        sim_poc_tx(poc_runtime_bytecode.clone(), &header,  &db, &storage_patch, initial_balance)?;

        db.flush();
        let (state_trie, storage_trie, contracts, block_hashes) = db.compact_data()?;

        assert_eq!(header.state_root, state_trie.hash());

        let vm_input = VmInput {
            header: header,
            state_trie: state_trie,
            storage_trie: storage_trie,
            contracts: contracts.into_iter().collect(),
            block_hashes: block_hashes.into_iter().collect(),
            poc_contract: poc_runtime_bytecode.bytecode,
            artifacts: Artifacts {
                initial_balance: initial_balance,
                storage: storage_patch,
            }
        };
        
        
        let segment_dir = tempdir().unwrap();
        let mut cycles = 0;
        let session = {
            let mut builder = ExecutorEnv::builder();
            let input = to_vec(&vm_input).expect("Could not serialize vm input");
            builder.session_limit(None)
            .write_slice(&input);
            
            if let Some(mut dump_input) = self.dump_input {
                dump_input.write_all(bytemuck::cast_slice(&input).as_ref())?;
            }

            let env = builder.build().unwrap();
            let mut exec = ExecutorImpl::from_elf(env, EVM_ELF).unwrap();
            exec.run_with_callback(|segment| {
                cycles += segment.cycles;
                Ok(Box::new(FileSegmentRef::new(&segment, segment_dir.path())?))
            })
            .unwrap()

        };
        // println!("IMAGE ID: {:x}", EVM_ID);
       
        let vm_output: VmOutput = session.journal.decode().unwrap();
        println!("{:?}", vm_output);
        println!("segments: {}, cycles: {}", session.segments.len(), cycles);

        if self.dry_run {
            return Ok(());
        }
        
        let start = Instant::now();
        let receipt = session.prove().unwrap();
        receipt.verify(EVM_ID)?;
        println!("proof time: {:?}", start.elapsed());
        
        let proof = Proof {
            version: env!("CARGO_PKG_VERSION").to_string(),
            image_id: EVM_ID,
            chain: "evm".to_string(),
            initial_balance: initial_balance,
            deals: deal_records,
            receipt: receipt,
        };
        proof.save(&mut self.output)?;
        Ok(())
    }
}
