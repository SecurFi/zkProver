use std::str::FromStr;
use alloy_primitives::{Address, U256};
use serde::{Serialize, Deserialize};
use anyhow::Result;

use crate::utils::parse_ether_value;

#[derive(Clone, Debug, Eq, PartialEq, Hash, Deserialize, Serialize)]
pub struct DealRecord {
    pub token: Address,
    pub balance: U256,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{0}")]
pub struct ParseDealError(String);


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
            token,
            balance,
        })
    }
}