use std::{str::FromStr,};
use eyre::Result;
use serde::{Serialize, Deserialize};
use ethers::{
    types::{Address, U256,},
    contract::{abigen,},
    abi::{AbiEncode},
};
use revm::{
    db::{DatabaseRef, CacheDB},
    primitives::{AccountInfo, Bytecode, TransactTo},
    EVM, DatabaseCommit,
};

use crate::{abi::parse_ether_value, inspectors::{CheatCodesInspector, CHEATCODE_ADDRESS}, runner::StateChangeset, utils::h160_to_b160, DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER};
abigen!(
    HSETUP,
    "artifacts/Setup.sol/Setup.json",
);


#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct ParseDealError(String);

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct DealRecord {
    pub address: Address,
    pub token: Address, // 0x0 means ETH
    pub balance: U256,
}

impl FromStr for DealRecord {
    type Err = ParseDealError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let err = || {
            ParseDealError(
                "deal record format must be `<token>:<balance>` or `<balance>`"
                    .to_string(),
            )
        };
        let mut iter = s.rsplit(':');
        let balance = iter.next().ok_or_else(err)?.trim().to_string();
        let balance = parse_ether_value(&balance).map_err(|_x| ParseDealError("error `<balance>`".to_string()))?;
        let token = iter.next().map(|x| Address::from_str(x).unwrap()).unwrap_or(Address::zero());
        Ok(DealRecord {
            address: Address::zero(),
            token,
            balance,
        })
    }
}


pub fn deal<D: DatabaseRef>(db: &D, rows: &Vec<DealRecord>) -> Result<StateChangeset> {

    let mut db = CacheDB::new(db);
    db.insert_account_info(DEFAULT_CONTRACT_ADDRESS.into(), AccountInfo::new(
        U256::zero().into(), 0, Bytecode::new_raw(HSETUP_DEPLOYED_BYTECODE.to_owned().0)
    ));
    db.insert_account_info(CHEATCODE_ADDRESS.into(), AccountInfo {
        code: Some(Bytecode::new_raw(vec![0u8].into()).to_checked()),
        ..Default::default()
    });

    let mut accounts: Vec<Address> = Vec::with_capacity(rows.len());
    let mut tokens: Vec<Address> = Vec::with_capacity(rows.len());
    let mut balances: Vec<U256> = Vec::with_capacity(rows.len());
    for row in rows {
        accounts.push(row.address.clone());
        tokens.push(row.token.clone());
        balances.push(row.balance.clone());
    }

    let call = hsetup::BatchDealCall{
        accounts, tokens, balances
    };
    let mut evm = EVM::new();
    evm.database(db);
    evm.env.tx.caller = DEFAULT_CALLER.into();
    evm.env.tx.transact_to = TransactTo::Call(DEFAULT_CONTRACT_ADDRESS.into());
    evm.env.tx.data = call.encode().into();


    let res = match evm.inspect(CheatCodesInspector::new()) {
        Ok(res) => res,
        Err(_e) => {
            eyre::bail!("deal error")
        }
    };
    if !res.result.is_success() {
        eyre::bail!("deal failed")
    }
    
    let mut state = res.state;
    for addr in [CHEATCODE_ADDRESS, Address::zero(), DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER] {
        state.remove(&h160_to_b160(addr));
    }

    for (k, v) in state.iter() {
        let changed_slot_count = v.storage.values().filter(|x| x.is_changed()).count();
        // check slot change
        if changed_slot_count > 1 {
            eyre::bail!("contract {} can not to deal", k);
        }
    }
    Ok(state)
    
}


pub fn deal_commit<D: DatabaseRef+DatabaseCommit>(db: &mut D, rows: &Vec<DealRecord>) -> Result<()> {
    let state = deal(db, rows)?;
    db.commit(state);
    Ok(())
}
