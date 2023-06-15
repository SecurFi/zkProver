use std::{
    path::PathBuf,
    fs::{File},
    io::{BufReader,}
};
use chains_evm::setup::DealRecord;
use clio::Output;
use risc0_zkvm::{
    SessionReceipt,
};
use serde::{Serialize, Deserialize};

#[derive(Debug, Deserialize, Serialize)]
pub struct Proof {
    pub version: String,
    pub image_id: [u32; 8],
    pub chain: String,
    pub raw_metadata: String,
    pub receipt: SessionReceipt,
    pub deals: Vec<DealRecord>,
}



impl Proof {
    pub fn load(path: impl Into<PathBuf>) -> eyre::Result<Self> {
        let path = path.into();
        let file = File::open(path)?;
        let file = BufReader::new(file);
        let data = bincode::deserialize_from(file)?;
        Ok(data)
    }

    pub fn save(&self, output: &mut Output) -> eyre::Result<()> {
        bincode::serialize_into(output, self)?;
        Ok(())
    }
}