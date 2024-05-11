use alloy_primitives::address;
use alloy_sol_types::SolCall;
use anyhow::{bail, Result};
use bridge::DEFAULT_CONTRACT_ADDRESS;
use revm::{
    db::CacheDB,
    primitives::{AccountInfo, Address, Bytecode, ExecutionResult, State, TransactTo, B256, KECCAK_EMPTY, U256},
    DatabaseCommit, DatabaseRef, Evm,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::helper_contract::{Helper, BALANCE_CHECKER_CONTRACT_CODE};

pub struct SafeStorageDB<'a, T: DatabaseRef> {
    db: &'a T,
    accounts: HashMap<Address, AccountInfo>,
}

impl<'a, T: DatabaseRef> SafeStorageDB<'a, T> {
    pub fn new(db: &'a T) -> Self {
        Self {
            db,
            accounts: HashMap::new(),
        }
    }

    pub fn insert_account_info(&mut self, address: Address, account: AccountInfo) {
        self.accounts.insert(address, account.clone());
    }
}

impl<'a, T: DatabaseRef> DatabaseRef for SafeStorageDB<'a, T> {
    type Error = T::Error;

    fn basic_ref(&self, address: Address) -> Result<Option<AccountInfo>, Self::Error> {
        match self.accounts.get(&address) {
            Some(account) => Ok(Some(account.clone())),
            None => {
                match self.db.basic_ref(address) {
                    Ok(account) => {
                        let mut account = account.unwrap();
                        if address == DEFAULT_CONTRACT_ADDRESS {
                            account.code_hash = KECCAK_EMPTY;
                        }
                        Ok(Some(account))
                    },
                    Err(_) => {
                        Ok(Some(AccountInfo::default()))
                    }
                }
            }
        }
    }

    fn code_by_hash_ref(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        self.db.code_by_hash_ref(code_hash)
    }

    fn storage_ref(&self, address: Address, index: U256) -> Result<U256, Self::Error> {
        self.db
            .storage_ref(address, index)
            .or_else(|_| Ok(U256::ZERO))
    }

    fn block_hash_ref(&self, number: U256) -> Result<B256, Self::Error> {
        self.db.block_hash_ref(number)
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AssetChange {
    pub address: Address,
    pub token: Address,
    pub from: U256,
    pub to: U256,
}

pub fn batch_get_token_balance<T: DatabaseRef>(
    db: &T,
    accounts: &Vec<Address>,
    tokens: &Vec<Address>,
)  -> Result<Vec<U256>> where <T as DatabaseRef>::Error: std::fmt::Debug {
    let mut db = SafeStorageDB::new(db);

    let caller_address = address!("1000000000000000000000000000000000000000");
    let contract_address = address!("2000000000000000000000000000000000000000");
    let bytecode = Bytecode::new_raw(BALANCE_CHECKER_CONTRACT_CODE.into());
    let account = AccountInfo::new(U256::ZERO, 0, bytecode.hash_slow(), bytecode);
    db.insert_account_info(contract_address, account);
    let mut evm = Evm::builder()
        .with_ref_db(db)
        .modify_tx_env(|tx| {
            tx.caller = caller_address;
            tx.transact_to = TransactTo::Call(contract_address);
            tx.data = Helper::balancesCall {
                users: accounts.clone(),
                tokens: tokens.clone(),
            }
            .abi_encode()
            .into();
        })
        .build();

    let result = match evm.transact_preverified() {
        Ok(result) => result.result,
        Err(err) => {
            println!("Failed to execute transaction: {:#?}", err);
            bail!("Failed to execute transaction")
        },
    };
    let ExecutionResult::Success { output, .. } = result else {
        bail!("Transaction failed");
    };

    let balances: Vec<U256> =
        Helper::balancesCall::abi_decode_returns(&output.into_data(), true)?._0;
    Ok(balances)
}

pub fn compute_asset_change<D: DatabaseRef>(
    accounts: &Vec<Address>,
    db: &D,
    state: State,
) -> Result<Vec<AssetChange>> where D::Error: std::fmt::Debug {
    let mut maybe_tokens: Vec<Address> = state
        .clone()
        .into_iter()
        .filter(|(_, info)| info.info.code.is_some())
        .map(|(address, _)| address)
        .collect();
    maybe_tokens.push(Address::ZERO);

    let origin = batch_get_token_balance(db, accounts, &maybe_tokens)?;

    let mut cache_db = CacheDB::new(db);
    cache_db.commit(state);

    let finial = batch_get_token_balance(&cache_db, accounts, &maybe_tokens)?;
    let mut result = Vec::new();
    for i in 0..origin.len() {
        let is_changed = origin[i] != finial[i];
        if is_changed {
            let account = accounts[i / maybe_tokens.len()];
            let token = maybe_tokens[i % maybe_tokens.len()];
            result.push(AssetChange {
                address: account,
                token: token,
                from: origin[i],
                to: finial[i],
            });
        }
    }
    Ok(result)
}
