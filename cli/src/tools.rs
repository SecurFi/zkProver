use std::io::Write;
use eyre::{Result, bail};
use clap::Parser;
use chains_evm::{
    compiler::compile_contract,
    opts::EvmOpts,
    runner::{create_db, POCRunner},
    setup::{deal_commit, DealRecord},
    DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER,
};
use bridge::{Env, ZkDb};
use ethers_core::types::{Address, U256};
use ethers_solc::{info::ContractInfo, utils::canonicalized, Artifact};
use risc0_zkvm::{serde::to_vec, Receipt};
use clio::{Output, Input};
use zk_methods::EVM_ID;

use crate::proof::Proof;


#[derive(Parser, Debug)]
pub struct PreArgs {
    contract: String,

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
    output: Output,

    #[clap(long, short, value_parser, default_value = "sketch_proof.bin")]
    proof: Output,

}


#[derive(Parser, Debug)]
pub struct PackArgs {
    #[clap(long, short, value_parser, default_value = "input.hex")]
    receipt: Input,

    #[clap(long, value_parser, default_value = "sketch_proof.bin")]
    proof: Input,

    #[clap(long, value_parser, default_value = "proof.bin")]
    output: Output,
}



impl PreArgs {
    pub async fn run(mut self) -> Result<()> {
        let path = canonicalized(self.contract).to_string_lossy().to_string();
        let contract_info = ContractInfo {
            path: Some(path), name: "Exploit".to_string()
        };
        let contract = compile_contract(&contract_info)?;
        let abi = contract.get_abi().unwrap().to_owned();
        let mut evm_opts = EvmOpts::default();
        evm_opts.fork_url = Some(self.rpc_url.clone());
        evm_opts.fork_block_number = self.block_number;

        let env = evm_opts.evm_env().await;

        let rpc_cache_dir = dirs_next::home_dir().expect("home dir not found").join(".0xhacked").join("cache").join("rpc");
        let cache_path =  rpc_cache_dir.join(format!("{}", env.cfg.chain_id)).join(format!("{}.db",env.block.number));

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
            bail!("execution failed, {:?}", result.reason);
        }
        println!("tx run success, gas used: {:?}", result.gas_used);

        if let Some(gas) = self.gas {
            if gas < result.gas_used {
                bail!("gas used exceed, limit: {}", gas);
            }
        }

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
        
        let mut v8bytes: Vec<u8> = Vec::new();
        v8bytes.extend_from_slice(bytemuck::cast_slice(&to_vec(&env).unwrap()));
        v8bytes.extend_from_slice(bytemuck::cast_slice(&to_vec(&zkdb).unwrap()));
        
        self.output.write_all(&v8bytes).unwrap();
        

        let proof = Proof {
            version: env!("CARGO_PKG_VERSION").to_string(),
            image_id: EVM_ID,
            chain: "ethereum".to_string(),
            raw_metadata: contract.raw_metadata.unwrap(),
            deals: deal_records,
            receipt: None,
        };
        proof.save(&mut self.proof).unwrap();

        return Ok(());
    }
}

impl PackArgs {
    pub fn run(self) -> Result<()> {

        let mut proof = Proof::load(self.proof)?;
        let receipt: Receipt = bincode::deserialize_from(self.receipt)?;
        receipt.verify(proof.image_id)?;
        proof.receipt = Some(receipt);
        proof.save(self.output)?;
        return Ok(());
    }
}