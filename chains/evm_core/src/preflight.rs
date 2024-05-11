use anyhow::{bail, Result};
use revm::primitives::{AccountInfo, Bytecode, ExecutionResult, TransactTo, U256, SpecId};
use revm::Evm;
use alloy_provider::{Network, Provider};
use alloy_transport::Transport;
use log::info;
use bridge::{ExploitInput, CALL_EXPLOIT_DATA, DEFAULT_CALLER, DEFAULT_CONTRACT_ADDRESS, DEFAULT_GAS_LIMIT};

use crate::block::BlockHeader;
use crate::db::{JsonBlockCacheDB, ProxyDB};


pub fn build_input<T, N, P>(
    contract: Bytecode,
    header: BlockHeader,
    rpc_db: &JsonBlockCacheDB<T, N, P>,
    initial_balance: U256,
) -> Result<ExploitInput>
where
T: Transport + Clone, N: Network, P: Provider<T, N>,
{
    let mut db = ProxyDB::new(rpc_db);
    // init account
    db.insert_account_info(
        DEFAULT_CONTRACT_ADDRESS,
        AccountInfo::new(initial_balance, 1, contract.hash_slow(), contract.clone()),
    );
    db.insert_account_info(DEFAULT_CALLER,  AccountInfo{
        nonce: 1, ..Default::default()
    });

    // apply patch
    // for (address, storage) in storage_patch.iter() {
    //     for (index, value) in storage {
    //         db.insert_account_storage(address.clone(), index.clone(), value.clone());
    //     }
    // }

    let block_env = header.into_block_env();
    let spec_id = SpecId::SHANGHAI;

    let mut evm = Evm::builder()
        .with_db(db)
        .with_spec_id(spec_id)
        .with_block_env(block_env.clone())
        .modify_tx_env(|tx| {
            tx.caller = DEFAULT_CALLER;
            tx.transact_to = TransactTo::Call(DEFAULT_CONTRACT_ADDRESS);
            tx.data = CALL_EXPLOIT_DATA;
            tx.value = U256::ZERO;
            tx.gas_limit = DEFAULT_GAS_LIMIT;
        })
        .build();

    let result_and_state = evm.transact_preverified()?;
    
    match result_and_state.result {
        ExecutionResult::Success{gas_used, ..} => {
            info!("Success! Gas used: {}", gas_used);
        }
        ExecutionResult::Revert {gas_used, ..} => {
            bail!("Revert, gas used: {}", gas_used)
        }
        ExecutionResult::Halt { reason, gas_used } => {
            bail!("Halt: {:#?}, gas used: {}", reason, gas_used)
        }
    }
    Ok(ExploitInput{
        db: evm.db().into_memdb(),
        block_env: block_env,
        spec_id: spec_id
    })
}
