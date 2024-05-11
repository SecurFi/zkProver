use std::io::{Read, Write};
use alloy_primitives::B256;
use anyhow::Result;
use serde::{Serialize, Deserialize};
use chains_evm_core::deal::DealRecord;
use risc0_zkvm::Receipt;


#[derive(Debug, Deserialize, Serialize)]
pub struct Proof {
    pub version: String,
    pub image_id: [u32; 8],
    pub chain_id: u64,
    pub spec_id: String,
    pub block_number: u64,
    pub poc_code_hash: B256,
    pub deals: Vec<DealRecord>,
    pub receipt: Option<Receipt>,
}



impl Proof {
    pub fn load<R: Read>(input: R) -> Result<Self> {
        let data = bincode::deserialize_from(input)?;
        Ok(data)
    }

    pub fn save<W: Write>(&self, output: W) -> Result<()> {
        bincode::serialize_into(output, self)?;
        Ok(())
    }
}