use anyhow::{Result, Context};
use alloy_primitives::U256;

#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::{Handle, Runtime};


#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
pub enum RuntimeOrHandle {
    Runtime(Runtime),
    Handle(Handle),
}

#[cfg(not(target_arch = "wasm32"))]
impl Default for RuntimeOrHandle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl RuntimeOrHandle {
    pub fn new() -> RuntimeOrHandle {
        match Handle::try_current() {
            Ok(handle) => RuntimeOrHandle::Handle(handle),
            Err(_) => RuntimeOrHandle::Runtime(Runtime::new().expect("Failed to start runtime")),
        }
    }

    pub fn block_on<F: std::future::Future>(&self, f: F) -> F::Output {
        match &self {
            RuntimeOrHandle::Runtime(runtime) => runtime.block_on(f),
            RuntimeOrHandle::Handle(handle) => tokio::task::block_in_place(|| handle.block_on(f)),
        }
    }
}


pub fn parse_ether_value(value: &str) -> Result<U256> {
    Ok(if value.starts_with("0x") {
        U256::from_str_radix(value, 16)?
    } else {
        alloy_dyn_abi::DynSolType::coerce_str(&alloy_dyn_abi::DynSolType::Uint(256), value)?
            .as_uint()
            .context("Could not parse ether value from string")?
            .0
    })
}