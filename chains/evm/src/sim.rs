use eyre::{bail, Result};
use revm::db::CacheDB;
use revm::primitives::{AccountInfo, BlockEnv, Bytecode, ExecutionResult, TransactTo, TxEnv};
use revm::{db::DatabaseRef, EVM};

use bridge::{get_specId_from_block_number, BlockHeader, DEFAULT_CALLER, DEFAULT_CONTRACT_ADDRESS};

use crate::deal::StoragePatch;
use crate::evm_primitives::{Bytes, U256};



pub fn sim_poc_tx<D>(
    contract: Bytecode,
    header: &BlockHeader,
    rpc_db: &D,
    storage_patch: &StoragePatch,
    initial_balance: U256,
) -> Result<()>
where
    D: DatabaseRef,
    <D as DatabaseRef>::Error: std::error::Error + Send + Sync +'static,
{
    let mut evm = EVM::new();
    let mut db = CacheDB::new(rpc_db);
    // init account
    db.insert_account_info(
        DEFAULT_CONTRACT_ADDRESS,
        AccountInfo::new(initial_balance, 1, contract.hash_slow(), contract),
    );
    db.insert_account_info(DEFAULT_CALLER,  AccountInfo{
        nonce: 1, ..Default::default()
    });

    // apply patch
    for (address, storage) in storage_patch {
        let db_account = db
            .load_account(*address)?;
        db_account
            .storage
            .extend(storage.into_iter().map(|(key, value)| (key, value)))
    }

    evm.database(db);
    // call exploit()
    let call_data = "0x63d9b770".parse::<Bytes>().unwrap();
    // tx env
    evm.env.tx = TxEnv {
        caller: DEFAULT_CALLER,
        transact_to: TransactTo::Call(DEFAULT_CONTRACT_ADDRESS),
        data: call_data,
        value: U256::ZERO,
        ..Default::default()
    };
    evm.env.block = BlockEnv {
        number: U256::from(header.number),
        timestamp: header.timestamp,
        ..Default::default()
    };
    evm.env.cfg.spec_id = get_specId_from_block_number(header.number);
    let result_and_state = evm.transact()?;

    match result_and_state.result {
        ExecutionResult::Success { gas_used, .. } => {
            if U256::from(gas_used) > header.gas_limit {
                bail!("tx gas limit exceeded");
            }
            println!("tx gas used: {}", gas_used);
        }
        ExecutionResult::Revert {gas_used, ..} => {
            bail!("Revert, gas used: {}", gas_used)
        }
        ExecutionResult::Halt { reason, gas_used } => {
            bail!("Halt: {:#?}, gas used: {}", reason, gas_used)
        }
    }
    Ok(())
}
