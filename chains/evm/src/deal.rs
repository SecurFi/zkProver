use alloy_sol_types::{sol, SolCall};
use eyre::{Result, bail};
use serde::{Serialize, Deserialize};
use std::collections::BTreeMap as Map;
use std::str::FromStr;
use revm::{
    db::{DatabaseRef, CacheDB},
    primitives::{AccountInfo, Bytecode, TransactTo,},
    EVM,
};
use bridge::{DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER};
use crate::utils::parse_ether_value;
use crate::evm_primitives::{Address, U256};
use crate::inspectors::{CHEATCODE_ADDRESS, CheatCodesInspector};
include!(concat!(env!("OUT_DIR"), "/deal_contract.rs"));


sol! {
    interface Deal {
        function batchDeal(address[] calldata accounts, address[] calldata tokens, uint256[] calldata balances) external;
    }
}



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
        let token = iter.next().map(|x| Address::from_str(x).unwrap()).unwrap_or(Address::default());
        Ok(DealRecord {
            address: Address::default(),
            token,
            balance,
        })
    }
}



pub type StoragePatch = Map<Address, Map<U256, U256>>;


pub fn deal<D: DatabaseRef>(db: &D, rows: &Vec<DealRecord>) -> Result<StoragePatch> 
where
    <D as DatabaseRef>::Error: std::fmt::Debug
{

    let mut db = CacheDB::new(db);
    let bytecode = Bytecode::new_raw(DEAL_CONTRACT_CODE.into());
    let account = AccountInfo::new(
        U256::from(0), 0, bytecode.hash_slow(), bytecode
    );
    db.insert_account_info(DEFAULT_CONTRACT_ADDRESS.into(), account);


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
    let call = Deal::batchDealCall {
        accounts, tokens, balances
    };
    let mut evm = EVM::new();
    evm.database(db);
    evm.env.tx.caller = DEFAULT_CALLER;
    evm.env.tx.transact_to = TransactTo::Call(DEFAULT_CONTRACT_ADDRESS);
    evm.env.tx.data = call.abi_encode().into();


    let res = match evm.inspect(CheatCodesInspector::new()) {
        Ok(res) => res,
        Err(e) => {
            bail!("deal tx error, {:?}", e)
        }
    };
    if !res.result.is_success() {
        bail!("deal tx failed")
    }
    
    let skip_addrs = vec![CHEATCODE_ADDRESS, Address::default(), DEFAULT_CONTRACT_ADDRESS, DEFAULT_CALLER];
    let mut storage_patch: StoragePatch = Map::new();
    for (addr, account) in res.state.iter() {
        if skip_addrs.contains(addr){
            continue;
        }
        let changed_slot_count = account.storage.values().filter(|x| x.is_changed()).count();
        // check slot change, now only support erc20 
        if changed_slot_count > 1 {
            bail!("contract {:#x} can not to deal", addr);
        }

        for (slot, diff) in account.storage.iter() {
            if diff.is_changed() {
                storage_patch
                   .entry(addr.clone())
                   .or_default()
                   .insert(slot.clone(), diff.present_value.clone());
            }
        }
    }

    Ok(storage_patch)
    
}
