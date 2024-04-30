use std::sync::Arc;

use ethers::{
    abi::Abi,
    types::{Address, Bytes, U256},
};
use revm::primitives::{
    Env, TransactTo,
    BlockEnv, TxEnv, AccountInfo,
    Log, ExecutionResult,
    Bytecode, U256 as rU256, B160, Account,
    SpecId,
};
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
        db: &'a mut DB,
        code: Bytes,
        contract: &'a Abi,
        initial_balance: Option<U256>,
        sender: Option<Address>,
        deployed_address: Option<Address>,
    ) -> Self {
        Self {
            env,
            db,
            code,
            contract,
            initial_balance: initial_balance.unwrap_or(U256::zero()),
            sender: sender.unwrap_or(DEFAULT_CALLER),
            deployed_address: deployed_address.unwrap_or(DEFAULT_CONTRACT_ADDRESS),
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
        let mut env = Env::default();
        env.tx = TxEnv {
            caller: self.sender.into(),
            transact_to: TransactTo::Call(self.deployed_address.into()),
            data: calldata.into(),
            value: rU256::ZERO,
            ..Default::default()
        };
        env.block = BlockEnv {
            number: self.env.block.number,
            timestamp: self.env.block.timestamp,
            ..Default::default()
        };
        env.cfg.spec_id = SpecId::SHANGHAI;

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
            ExecutionResult::Halt { reason: a, gas_used } => {
                println!("halt reason: {:?}", a);
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
    let db = BlockchainDb::new(meta, cache_path.map(|x|x.into()));

    let number: U256 = env.block.number.into();
    let backend = SharedBackend::spawn_backend(Arc::new(provider), db.clone(), Some(number.as_u64().into())).await;
    ForkedDatabase::new(backend, db)
}
