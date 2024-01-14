use ethers_providers::Middleware;
use eyre::{bail, Result};
use revm::primitives::{AccountInfo, BlockEnv, Bytecode, ExecutionResult, TransactTo, TxEnv};
use revm::EVM;

use bridge::{get_specId_from_block_number, BlockHeader, DEFAULT_CALLER, DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALL_DATA, VmInput, Artifacts};

use crate::db::{JsonBlockCacheDB, ProxyDb};
use crate::deal::StoragePatch;
use crate::evm_primitives::U256;



pub fn build_vminput<'a, M>(
    contract: Bytecode,
    header: BlockHeader,
    rpc_db: &JsonBlockCacheDB<'a, M>,
    storage_patch: StoragePatch,
    initial_balance: U256,
    author: [u8; 20]
) -> Result<VmInput>
where
    M: Middleware +'static,
{
    let mut evm = EVM::new();
    let mut db = ProxyDb::new(rpc_db);
    // init account
    db.insert_account_info(
        DEFAULT_CONTRACT_ADDRESS,
        AccountInfo::new(initial_balance, 1, contract.hash_slow(), contract.clone()),
    );
    db.insert_account_info(DEFAULT_CALLER,  AccountInfo{
        nonce: 1, ..Default::default()
    });

    // apply patch
    for (address, storage) in storage_patch.iter() {
        for (index, value) in storage {
            db.insert_account_storage(address.clone(), index.clone(), value.clone());
        }
    }

    evm.database(db);
    // call exploit()
    // tx env
    evm.env.tx = TxEnv {
        caller: DEFAULT_CALLER,
        transact_to: TransactTo::Call(DEFAULT_CONTRACT_ADDRESS),
        data: DEFAULT_CALL_DATA,
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
    db = evm.take_db();
    let (state_trie, storage_trie, contracts, block_hashes) = db.compact_trace_data()?;
    assert_eq!(header.state_root, state_trie.hash());
    Ok(VmInput {
        header: header,
        state_trie: state_trie,
        storage_trie: storage_trie,
        contracts: contracts.into_iter().collect(),
        block_hashes: block_hashes.into_iter().collect(),
        poc_contract: contract.bytecode,
        author: author,
        artifacts: Artifacts {
            initial_balance: initial_balance,
            storage: storage_patch,
        }
    })
}
