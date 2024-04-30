use std::str::FromStr;
use hashbrown::HashMap;
use ethers::{
    contract::{abigen},
    types::{Address, U256,},
    abi::{AbiEncode, AbiDecode},
};
use revm:: {
    EVM, db::{DatabaseRef, CacheDB, DatabaseCommit},
    primitives::{Account, AccountInfo, B160, Bytecode, TransactTo, ExecutionResult, B256, U256 as rU256}
};
use serde::{Serialize, Deserialize};
use eyre::{Result};


abigen!(BalanceChecker, "artifacts/BalanceChecker.sol/BalanceChecker.json");


pub type StateChangeset = HashMap<B160, Account>;


pub struct SafeStorageDB<'a, T: DatabaseRef> {
    db: &'a T,
    accounts: HashMap<B160, AccountInfo>,
}

impl<'a, T: DatabaseRef> SafeStorageDB<'a, T> {
    pub fn new(db: &'a T) -> Self {
        Self { db, accounts: HashMap::new() }
    }

    pub fn insert_account_info(&mut self, address: B160, account: AccountInfo) {
        self.accounts.insert(address, account.clone());
    }
}


impl<'a, T: DatabaseRef> DatabaseRef for SafeStorageDB<'a, T> {
    type Error = T::Error;

    fn basic(&self, address: B160) -> Result<Option<AccountInfo>, Self::Error> {
        match self.accounts.get(&address) {
            Some(account) => Ok(Some(account.clone())),
            None => self.db.basic(address)
        }
    }

    fn code_by_hash(&self, code_hash: B256) -> Result<Bytecode, Self::Error> {
        // self.db.code_by_hash(code_hash)
        self.db.code_by_hash(code_hash)
    }

    fn storage(&self, address: B160, index: rU256) -> Result<rU256, Self::Error> {
        self.db.storage(address, index).or_else(|_| Ok(rU256::ZERO))
    }

    fn block_hash(&self, number: rU256) -> Result<B256, Self::Error> {
        self.db.block_hash(number)
    }
}


pub fn batch_get_token_balance<T: DatabaseRef>(
    db: &T, accounts: &Vec<Address>, tokens: &Vec<Address>) -> Result<Vec<U256>> {
    let mut db = SafeStorageDB::new(db);
    let mut evm = EVM::new();

    let caller_address = B160::from_str("0x1000000000000000000000000000000000000000").unwrap();
    let contract_address = B160::from_str("0x2000000000000000000000000000000000000000").unwrap();

    db.insert_account_info(contract_address, AccountInfo::new(
        U256::zero().into(), 1, Bytecode::new_raw(BALANCECHECKER_DEPLOYED_BYTECODE.to_owned().0)
    ));
    evm.database(db);

    evm.env.tx.caller = caller_address;
    evm.env.tx.transact_to = TransactTo::Call(contract_address);
    
    let call = balance_checker::BalancesCall{
        users: accounts.clone(),
        tokens: tokens.clone(),
    };
    evm.env.tx.data = call.encode().into();

    let result_and_state = match evm.transact_ref() {
        Ok(result) => result,
        Err(_) => {
            eyre::bail!("get balance error");
        }
    };
    match result_and_state.result {
        ExecutionResult::Success { output , ..} => {
            let result: Vec<U256> = balance_checker::BalancesReturn::decode(&output.into_data()).unwrap().0;
            Ok(result)
        },
        _ => {
            eyre::bail!("Faield to get balance");
        }
    }

    
}


#[derive(Clone, Debug,  Default, Serialize, Deserialize)]
pub struct AssetChange {
    pub address: Address,
    pub token: Address,
    pub from: U256,
    pub to: U256,
}

pub fn get_asset_change_form_db_state<D: DatabaseRef>(accounts: &Vec<Address>, db: &D, state_changeset: StateChangeset) -> Result<Vec<AssetChange>> {
    let mut tokens: Vec<Address>= state_changeset
        .clone()
        .iter()
        .filter(|(_, info)| info.info.code.is_some())
        .map(|(address, _)| Address::from(*address)).collect();


    tokens.push(Address::zero());
    let before = batch_get_token_balance(db, accounts, &tokens)?;

    let mut cache_db = CacheDB::new(db);
    cache_db.commit(state_changeset);     

    let after = batch_get_token_balance(&cache_db, &accounts, &tokens)?;

    let mut result = Vec::new();
    for i in 0..before.len() {
        let is_changed = before[i] != after[i];
        if is_changed {
            let account = accounts[i/tokens.len()];
            let token = tokens[i%tokens.len()];
            result.push(AssetChange{
                address: account,
                token: token,
                from: before[i],
                to: after[i],
            });
        }
    }
    Ok(result)
}
