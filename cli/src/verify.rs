use std::collections::HashSet;
use std::ops::{Deref, DerefMut};
use std::collections::HashMap;
use chains_evm::balance::{get_asset_change_form_db_state, AssetChange};
use clap::Parser;
use clio::{Input, Output};
use eyre::Result;
use serde::{Deserialize, Serialize};
use bridge::{EvmResult, TransactTo, B160, U256 as rU256, B256, KECCAK_EMPTY};
use chains_evm::{
    fork::database::DatabaseRef,
    opts::EvmOpts,
    runner::create_db,
    setup::{deal, DealRecord},
    utils::ru256_to_u256,
    Address,
};
use crate::proof::Proof;
use zk_methods::EVM_ID;



#[derive(Parser, Debug)]
pub struct VerifyArgs {
    /// proof file
    path: Input,

    /// Output file
    #[clap(long, short, value_parser, default_value = "-")]
    output: Output,

    #[clap(short, long)]
    rpc_url: String,
}


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
    pub balance: Delta<rU256>,
    pub nonce: Delta<u64>,
    pub code_hash: Delta<B256>,
    pub storage: HashMap<rU256, Delta<rU256>>,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct VerifyResult {
    pub version: String,
    pub chain: String,
    pub deals: Vec<DealRecord>,
    pub state_diff: StateDiff,
    pub asset_change: Vec<AssetChange>,
    pub block_number: u64,
    pub gas_used: u64,
}


async fn verify(proof: Proof, rpc_url: String) -> Result<VerifyResult> {
    let evm_result: EvmResult = proof.receipt.unwrap().journal.decode()?;

    let block_number = ru256_to_u256(evm_result.env.block.number).as_u64();
    let mut evm_opts = EvmOpts::default();
    evm_opts.fork_url = Some(rpc_url.clone());
    evm_opts.fork_block_number = Some(block_number);
    let mut env = evm_opts.evm_env().await;
    let db = create_db(env.clone(), rpc_url.clone(), None).await;

    env.block.basefee = rU256::ZERO;
    if evm_result.env.block != evm_result.env.block {
        eyre::bail!("Block info check failed")
    }
    let contract_address = match evm_result.env.tx.transact_to {
        TransactTo::Call(address) => address,
        _ => panic!("Unexpected transact_to"),
    };
    let caller = evm_result.env.tx.caller;

    // check account
    let mut initial_balance = rU256::ZERO;
    for record in proof.deals.iter() {
        if record.token == Address::zero() {
            initial_balance = record.balance.into();
            break;
        }
    }
    if evm_result
        .db
        .accounts
        .get(&contract_address)
        .unwrap()
        .balance
        != initial_balance
    {
        eyre::bail!("Contract init balance check failed")
    }
    if evm_result.db.accounts.get(&caller).unwrap().balance != rU256::ZERO {
        eyre::bail!("Caller init balance check failed")
    }
    for (address, account) in evm_result.db.accounts.iter() {
        if address != &contract_address && address != &caller {
            let trust_account = db.basic(*address).unwrap().expect("account not found");
            if trust_account != *account {
                eyre::bail!("Account info check failed")
            }
        }
    }

    // check storage
    let deal_address_slot: HashMap<B160, HashSet<rU256>> = HashMap::new();
    let deal_state_change = deal(&db, &proof.deals)?;
    for (address, account) in deal_state_change.iter() {
        for (key, sslot) in account.storage.iter() {
            if sslot.is_changed() {
                if *evm_result
                    .db
                    .storage
                    .get(address)
                    .unwrap()
                    .get(key)
                    .unwrap()
                    != sslot.present_value
                {
                    eyre::bail!("deal info check failed")
                }
            }
        }
    }
    for (address, storage_info) in evm_result.db.storage.iter() {
        for (slot, value) in storage_info.iter() {
            if deal_address_slot.contains_key(address)
                && deal_address_slot.get(address).unwrap().contains(slot)
            {
                continue;
            }
            let trust_value = db.storage(*address, *slot).expect("storage not found");
            if trust_value != *value {
                eyre::bail!("Storage info check failed")
            }
        }
    }

    let mut state_diff = StateDiff::default();

    for (address, account) in evm_result.state.iter() {
        let mut nonce_delta: Delta<u64> = Delta::default();
        let mut balance_delta: Delta<rU256> = Delta::default();
        let mut storage_delta: HashMap<rU256, Delta<rU256>> = HashMap::new();
        let before_account = evm_result.db.accounts.get(address);
        if before_account.is_none() || before_account.unwrap().is_empty() {
            balance_delta = Delta::Added(account.info.balance);
            nonce_delta = Delta::Added(account.info.nonce);
            let acc = state_diff.entry((*address).into()).or_default();
            acc.balance = balance_delta;
            acc.nonce = nonce_delta;
            if account.info.code_hash != KECCAK_EMPTY {
                acc.code_hash = Delta::Added(account.info.code_hash);
            }
            
            continue;
        }
        let before_account = before_account.unwrap();
        if account.is_destroyed {
            balance_delta = Delta::Removed(before_account.balance);
            nonce_delta = Delta::Removed(before_account.nonce);
            let acc = state_diff.entry((*address).into()).or_default();
            acc.balance = balance_delta;
            acc.nonce = nonce_delta;
            continue;
        }

        for (key, sslot) in account.storage.iter() {
            if !sslot.is_changed() {
                continue;
            }
            let before_value = evm_result.db.storage.get(address);
            if before_value.is_none() {
                storage_delta.insert(*key, Delta::Added(sslot.present_value));
            } else {
                storage_delta.insert(
                    *key,
                    Delta::Changed(ChangedType {
                        from: sslot.original_value,
                        to: sslot.present_value,
                    }),
                );
            }
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

    let accounts: Vec<Address> = evm_result
        .db
        .accounts
        .keys()
        .map(|x| x.clone().into())
        .collect();
    // fix the code of account in evm_result.state
    let mut state = evm_result.state;
    for (address, account) in state.iter_mut() {
        let code = evm_result.db.basic(*address).unwrap().unwrap().code;
        account.info.code = code;
        
    }
    let asset_change = get_asset_change_form_db_state(&accounts, &evm_result.db, state)?;
    Ok(VerifyResult {
        version: proof.version,
        chain: proof.chain,
        deals: proof.deals,
        state_diff: state_diff,
        asset_change: asset_change,
        block_number: block_number,
        gas_used: evm_result.gas_used,
    })
}


impl VerifyArgs {
    pub async fn run(self) -> Result<()> {
        let proof = Proof::load(self.path)?;
        proof.receipt.clone().unwrap().verify(EVM_ID)?;
        let result = verify(proof, self.rpc_url).await?;

        serde_json::to_writer(self.output, &result)?;
        Ok(())
    }
}