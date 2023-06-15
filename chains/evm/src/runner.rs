use std::{sync::Arc,};

use ethers::{
    abi::{Abi},
    types::{Address, Bytes, U256},
};
use revm::{primitives::{
    Env, TransactTo,
    BlockEnv, TxEnv, AccountInfo,
    Log, ExecutionResult,
    Bytecode, U256 as rU256, B160, Account
}};
use hashbrown::HashMap;
use revm::EVM;

use crate::{
    fork::{
        BlockchainDbMeta, BlockchainDb, SharedBackend, 
        database::{ForkedDatabase,Db}
    }, provider::get_http_provider, decode::decode_revert, DEFAULT_CALLER, DEFAULT_CONTRACT_ADDRESS};


/// A mapping of addresses to their changed state.
pub type StateChangeset = HashMap<B160, Account>;

#[derive(Debug)]
pub struct RunResult{
    pub success: bool,
    pub reason: Option<String>,
    pub gas_used: u64,
    pub logs: Vec<Log>,
    pub state_changes: Option<StateChangeset>,
    pub env: Env,
}


pub struct POCRunner<'a, Db: ?Sized> {
    env: Env,
    gas_limit: Option<U256>,
    pub db: &'a mut Db,
    pub code: Bytes,

    pub contract: &'a Abi,

    pub initial_balance: U256,

    pub sender: Address,

    pub deployed_address: Address,
}

impl<'a, DB: Db + ?Sized> POCRunner<'a, DB> {
    
    pub fn new(
        env: Env,
        gas_limit: Option<U256>,
        db: &'a mut DB,
        code: Bytes,
        contract: &'a Abi,
        initial_balance: Option<U256>,
        sender: Option<Address>,
        deployed_address: Option<Address>,
    ) -> Self {
        Self {
            env,
            gas_limit,
            db,
            code,
            contract,
            initial_balance: initial_balance.unwrap_or(U256::zero()),
            sender: sender.unwrap_or(DEFAULT_CALLER),
            deployed_address: deployed_address.unwrap_or(DEFAULT_CONTRACT_ADDRESS),
        }
    }

    fn build_test_env(
        &self,
        caller: Address,
        transact_to: TransactTo,
        data: Bytes,
        value: U256,
    ) -> Env {
        let gas_limit = self.gas_limit.unwrap_or(self.env.block.gas_limit.into());
        Env {
            cfg: self.env.cfg.clone(),
            block: BlockEnv {
                basefee: U256::zero().into(),
                gas_limit: gas_limit.into(),
                ..self.env.block.clone()
            },
            tx: TxEnv {
                caller: caller.into(),
                transact_to,
                data: data.0,
                value: value.into(),
                gas_price: U256::zero().into(),
                gas_priority_fee: None,
                gas_limit: gas_limit.as_u64(),
                ..self.env.tx.clone()
            },
        }
    }

    pub fn setup(&mut self) {
        self.db.insert_account(
            self.deployed_address,
            AccountInfo::new(
                self.initial_balance.into(), 1, Bytecode::new_raw(self.code.0.clone())
            )
        );
        self.db.insert_account(self.sender, AccountInfo::new(
            rU256::ZERO, 0, Bytecode::default()
        ));
    }

    pub fn run(&mut self) -> eyre::Result<RunResult> {
        let calldata = self.contract.function("exploit").unwrap().encode_input(&[]).unwrap();
        let env = self.build_test_env(
            self.sender,
            TransactTo::Call(self.deployed_address.into()),
            calldata.into(),
            U256::zero(),
        );
        let mut evm = EVM::with_env(env.clone());
        evm.database(&mut self.db);
        let result_and_state = match evm.transact() {
            Ok(result) => result,
            Err(_) => {
                eyre::bail!("Failed to execute transaction");
            }
        };
        match result_and_state.result {
            ExecutionResult::Success { reason: _, gas_used, gas_refunded: _, logs, output: _ } => {
                let state_changes = Some(result_and_state.state);
                Ok(RunResult {
                    success: true,
                    reason: None,
                    gas_used: gas_used,
                    logs,
                    state_changes,
                    env,
                })
            }
            ExecutionResult::Halt { reason: _, gas_used } => {
                // println!("halt reason: {:?}", reason);
                Ok(RunResult {
                    success: false, 
                    reason: None, 
                    gas_used: gas_used, 
                    logs: vec![], 
                    state_changes: None,
                    env
                })
            }
            ExecutionResult::Revert {gas_used, output} => {
                let reason = decode_revert(&output, None, None).unwrap_or_else(|_| "Unknown".to_string());
                // println!("revert reason: {:?}, output: {:?}", reason, output);
                Ok(RunResult {
                    success: false, 
                    reason: Some(reason),
                    gas_used: gas_used, 
                    logs: vec![], 
                    state_changes: None,
                    env
                })
            }
        }

    }


}



pub async fn create_db(env: Env, url: String, cache_path: Option<String>) -> ForkedDatabase {
    let provider = get_http_provider(url.clone());
    let meta = BlockchainDbMeta::new(env.clone(), url);

    // let cache_path = format!("mainnet-cache-{}.db", env.block.number);    
    let db = BlockchainDb::new(meta, cache_path.map(|path| path.into()));

    let number: U256 = env.block.number.into();
    let backend = SharedBackend::spawn_backend(Arc::new(provider), db.clone(), Some(number.as_u64().into())).await;
    ForkedDatabase::new(backend, db)
}
