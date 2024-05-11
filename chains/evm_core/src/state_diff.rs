use std::collections::HashMap;
use std::ops::{Deref, DerefMut};
use alloy_primitives::{U256, B256, Address};
use bridge::MemDB;
use revm::primitives::State;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChangedType<T> {
    pub from: T,
    pub to: T,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Delta<T> {
    #[default]
    #[serde(rename = "=")]
    Unchanged,
    #[serde(rename = "+")]
    Added(T),
    #[serde(rename = "-")]
    Removed(T),
    #[serde(rename = "*")]
    Changed(ChangedType<T>),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountDiff {
    pub balance: Delta<U256>,
    pub nonce: Delta<u64>,
    pub code_hash: Delta<B256>,
    pub storage: HashMap<U256, Delta<U256>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StateDiff(pub HashMap<Address, AccountDiff>);

impl Deref for StateDiff {
    type Target = HashMap<Address, AccountDiff>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for StateDiff {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}


pub fn compute_state_diff(state: &State, db: &MemDB) -> StateDiff {
    let mut state_diff = StateDiff::default();

    for (address, account) in state.iter() {
        let mut nonce_delta: Delta<u64> = Delta::default();
        let mut balance_delta: Delta<U256> = Delta::default();
        let mut storage_delta: HashMap<U256, Delta<U256>> = HashMap::new();

        let before_account = db.accounts.get(address);
        if before_account.is_none() || before_account.unwrap().info.is_empty() {
            balance_delta = Delta::Added(account.info.balance);
            nonce_delta = Delta::Added(account.info.nonce);

            let acc = state_diff.entry(address.clone()).or_default();
            acc.balance = balance_delta;
            acc.nonce = nonce_delta;

            if !account.info.is_empty_code_hash() {
                acc.code_hash = Delta::Added(account.info.code_hash());
            }
            continue;
        }

        let before_account = before_account.unwrap();
        if account.is_selfdestructed() {
            balance_delta = Delta::Removed(before_account.info.balance);
            nonce_delta = Delta::Removed(before_account.info.nonce);
        } else {
            if account.info.balance != before_account.info.balance {
                balance_delta = Delta::Changed(ChangedType { from: before_account.info.balance, to: account.info.balance });
            }
            if account.info.nonce != before_account.info.nonce {
                nonce_delta = Delta::Changed(ChangedType { from: before_account.info.nonce, to: account.info.nonce });
            }
        }

        for (key, sslot) in account.storage.iter() {
            if !sslot.is_changed() {
                continue;
            }
            storage_delta.insert(
                key.clone(),
                Delta::Changed(ChangedType {
                    from: sslot.original_value(),
                    to: sslot.present_value()
                })
            );
        }
        
        if let Delta::Unchanged = balance_delta {
            if let Delta::Unchanged = nonce_delta {
                if storage_delta.is_empty() {
                    continue;
                }
            }
        }

        let acc = state_diff.entry((*address).into()).or_default();
        acc.balance = balance_delta;
        acc.nonce = nonce_delta;
        acc.storage = storage_delta;

    }

    state_diff
}