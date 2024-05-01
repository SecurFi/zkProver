use crate::proof::Proof;
use bridge::{Env, ZkDb};
use chains_evm::{
    compiler::compile_contract,
    opts::EvmOpts,
    runner::{create_db, POCRunner},
    setup::{deal_commit, DealRecord},
    DEFAULT_CALLER, DEFAULT_CONTRACT_ADDRESS,
};
use clap::Parser;
use clio::Output;
use ethers_core::types::{Address, U256};
use ethers_solc::Artifact;
use eyre::{bail, Result};
use risc0_zkvm::{ExecutorEnv, ExecutorImpl};
use std::time::Instant;

#[cfg(feature = "prover")]
use zk_methods::{EVM_ELF, EVM_ID};

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

    /// Output file
    #[clap(long, short, value_parser, default_value = "proof.bin")]
    output: Output,
}

impl EvmArgs {
    /// Executes the `evm` subcommand.
    pub async fn run(mut self) -> Result<()> {
        let contract = compile_contract(self.contract)?;
        let abi = contract.get_abi().unwrap().to_owned();
        let mut evm_opts = EvmOpts::default();
        evm_opts.fork_url = Some(self.rpc_url.clone());
        evm_opts.fork_block_number = self.block_number;

        let env: Env = evm_opts.evm_env().await;
        let rpc_cache_dir = dirs_next::home_dir()
            .expect("home dir not found")
            .join(".SecurFi")
            .join("cache")
            .join("rpc");
        let cache_path = rpc_cache_dir
            .join(format!("{}", env.cfg.chain_id))
            .join(format!("{}.db", env.block.number));

        let mut db = create_db(
            env.clone(),
            self.rpc_url.clone(),
            cache_path.to_str().map(|x| x.into()),
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
            bail!("execution failed, {:?}", result.reason);
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

        let evm_id: Vec<u8> = EVM_ID.iter().flat_map(|x| x.to_le_bytes()).collect();
        #[cfg(feature = "prover")]
        {
            let start = Instant::now();
            // let segment_limit_po2 = 22;
            let zk_env = ExecutorEnv::builder()
                .write(&env)
                .unwrap()
                .write(&zkdb)
                .unwrap()
                // .segment_limit_po2(segment_limit_po2)
                // .session_limit(None)
                .build()
                .unwrap();

            let mut exec = ExecutorImpl::from_elf(zk_env, EVM_ELF).unwrap();

            let session = exec.run().unwrap();

            // println!(
            //     "Executor ran in (roughly) {} cycles",
            //     session.segments.len() * (1 << segment_limit_po2)
            // );

            if !self.dry_run {
                println!(
                    "starting generate zk proof, image id: {}",
                    hex::encode(evm_id)
                );
                let receipt = session.prove().unwrap();
                receipt.verify(EVM_ID)?;
                let proof = Proof {
                    version: env!("CARGO_PKG_VERSION").to_string(),
                    image_id: EVM_ID,
                    chain: "ethereum".to_string(),
                    raw_metadata: contract.raw_metadata.unwrap(),
                    deals: deal_records,
                    receipt: Some(receipt),
                };
                proof.save(&mut self.output).unwrap();
                let duration = start.elapsed();
                println!("Time elapsed is: {:?}", duration);
            }
        };
        Ok(())
    }
}

// const NULL_SEGMENT_REF: NullSegmentRef = NullSegmentRef {};
// #[derive(Serialize, Deserialize)]
// struct NullSegmentRef {}

// impl SegmentRef for NullSegmentRef {
//     fn resolve(&self) -> anyhow::Result<Segment> {
//         unimplemented!()
//     }
// }
