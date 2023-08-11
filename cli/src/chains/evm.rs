use std::{time::Instant};

use crate::{proof::Proof, VERSION_MESSAGE};
use bridge::{Env, ZkDb, EvmInput};
use chains_evm::{
    compiler::compile_contract,
    opts::EvmOpts,
    runner::{create_db, POCRunner},
    setup::{deal_commit, DealRecord},
    DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER,
};
use clap::Parser;
use clio::Output;
use ethers_core::types::{Address, U256};
use ethers_solc::{info::ContractInfo, utils::canonicalized, Artifact};
use eyre::Result;
use risc0_zkvm::{serde::to_vec, Executor, ExecutorEnv, FileSegmentRef};

use tempfile::tempdir;
#[cfg(feature = "prover")]
use zk_methods::{EVM_ELF, EVM_ID};


#[derive(Parser, Debug)] // requires `derive` feature
pub struct EvmArgs {
    /// The contract identifier in the form `<path>:<contractname>`.
    contract: ContractInfo,

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

    /// Output file 
    #[clap(long, short, value_parser, default_value = "proof.bin")]
    output: Output,
}

impl EvmArgs {
    /// Executes the `evm` subcommand.
    pub async fn run(mut self) -> Result<()> {
        if let Some(ref mut path) = self.contract.path {
            *path = canonicalized(path.to_string())
                .to_string_lossy()
                .to_string();
        }
        let contract = compile_contract(&self.contract).unwrap();
        let abi = contract.get_abi().unwrap().to_owned();
        let mut evm_opts = EvmOpts::default();
        evm_opts.fork_url = Some(self.rpc_url.clone());
        evm_opts.fork_block_number = self.block_number;

        let env = evm_opts.evm_env().await;

        let mut db = create_db(
            env.clone(),
            self.rpc_url.clone(),
            Some(format!("mainnet-cache-{}.db", env.block.number)),
        )
        .await;

        let deal_records: Vec<DealRecord> = self
            .deal
            .unwrap_or_default()
            .iter()
            .map(|x| DealRecord {
                address: DEFAULT_CONTRACT_ADDRESS,
                ..x.clone()
            })
            .collect();
        
        if deal_records.len() > 0 {
            deal_commit(&mut db, &deal_records)?;
        }

        let mut initial_balance = U256::zero();
        for record in deal_records.iter() {
            if record.token == Address::zero() {
                initial_balance = record.balance;
                break;
            }
        }

        let deployed_bytecode = contract.get_deployed_bytecode().unwrap().into_owned();
        let mut runner = POCRunner::new(
            env.clone(),
            None,
            &mut db,
            deployed_bytecode
                .bytecode
                .unwrap()
                .object
                .into_bytes()
                .unwrap(),
            &abi,
            Some(initial_balance),
            Some(DEFAULT_CALLER),
            Some(DEFAULT_CONTRACT_ADDRESS),
        );

        runner.setup();
        let result = runner.run()?;

        db.flush_cache();
        
        if !result.success {
            println!("execution failed, reason: {:?}", result.reason);
            return Ok(());
        }

        println!("tx run success, gas used: {:?}", result.gas_used);
        let mut zkdb = ZkDb::default();
        db.database()
            .accounts
            .iter()
            .for_each(|(address, account)| {
                zkdb.accounts.insert(*address, account.info.clone());
                zkdb.storage.insert(*address, account.storage.clone());
            });
        zkdb.block_hashes = db.database().block_hashes.clone();

        let mut env = Env::default();

        env.block = result.env.block.clone();
        env.tx = result.env.tx.clone();
        env.cfg.chain_id = result.env.cfg.chain_id.clone();
        env.cfg.spec_id = result.env.cfg.spec_id.clone();

        if self.dry_run {
            return Ok(());
        }

        let evm_id: Vec<u8> = EVM_ID.iter().flat_map(|x| x.to_be_bytes()).collect();
        #[cfg(feature = "prover")]
        {
            println!(
                "starting generate zk proof, image id: {}",
                hex::encode(evm_id)
            );
            let start = Instant::now();
            let input = EvmInput {
                env: env,
                db: zkdb,
            };
            let env = ExecutorEnv::builder()
                .add_input(&to_vec(&input).unwrap())
                .session_limit(1024 * 1024 * 1024)
                .build();

            let mut exec = Executor::from_elf(env, EVM_ELF).unwrap();

            let segment_dir = tempdir().unwrap();
            let session = exec
                .run_with_callback(|segment| {
                    println!("proof segment{}: cycles: {:?}", segment.index, segment.insn_cycles);
                    Ok(Box::new(FileSegmentRef::new(
                        &segment,
                        &segment_dir.path(),
                    )?))
                })
                .unwrap();
            let receipt = session.prove().unwrap();

            receipt.verify(EVM_ID)?;

            let proof = Proof {
                chain: "ethereum".to_string(),
                raw_metadata: contract.raw_metadata.unwrap(),
                version: VERSION_MESSAGE.to_string(),
                deals: deal_records,
                image_id: EVM_ID,
                receipt,
            };
            proof.save(&mut self.output).unwrap();
            let duration = start.elapsed();
            println!("Time elapsed is: {:?}", duration);
        };
        Ok(())
    }
}
