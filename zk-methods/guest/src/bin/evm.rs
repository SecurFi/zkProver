#![no_main]
#![allow(unused_imports)]

use bridge::{ZkDb, EVM, Env, EvmResult, ExecutionResult, TransactTo};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

pub fn main() {
    let env: Env = env::read();
    let db: ZkDb = env::read();
    let mut evm = EVM::new();
    evm.database(db);
    evm.env = env;
    let res = evm.transact().unwrap();
    if let ExecutionResult::Success{gas_used, ..} = res.result {
        let contract_address =  match evm.env.tx.transact_to {
            TransactTo::Call(address) => address,
            _ => panic!("Unexpected transact_to"),
        };
        let mut db = evm.take_db();

        // clear code
        let mut state = res.state;
        state.iter_mut().for_each(|(_, account)| {
            account.info.code = None;
        });
        
        let contract_info = db.accounts.get_mut(&contract_address).unwrap();
        contract_info.code = None;

        env::commit(&EvmResult {
            state: state,
            db: db,
            gas_used: gas_used,
            env: evm.env,
        });
    } else {
        panic!("tx run failed");
    }
}
