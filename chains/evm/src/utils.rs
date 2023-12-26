use eyre::{Result, bail};
#[cfg(not(target_arch = "wasm32"))]
use tokio::runtime::{Handle, Runtime};
use crate::evm_primitives::U256;


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


/// Parses an ether value from a string.
///
/// The amount can be tagged with a unit, e.g. "1ether".
///
/// If the string represents an untagged amount (e.g. "100") then
/// it is interpreted as wei.
pub fn parse_ether_value(value: &str) -> Result<U256> {
    Ok(if value.starts_with("0x") {
        U256::from_str_radix(value, 16)?
    } else {
        
        let mut split_idx = 0;
        for (idx, c) in value.chars().enumerate() {
            if !c.is_numeric() && c!= '.' {
                split_idx = idx;
                break;
            }
        }

        if split_idx > 0 {
            let (num, unit) = value.split_at(split_idx);
            let num = num.parse::<f64>()?;
            let unit = match unit.to_lowercase().as_str() {
                "wei" => 1 as f64,
                "gwei" => 1e9 as f64,
                "ether" => 1e18 as f64,
                _ => bail!("Invalid unit")
            };
            U256::from(num * unit)
        } else {
            U256::from_str_radix(value, 10)?
        }
    })
}





#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse() {
        let v = parse_ether_value("1ether").unwrap();
        assert_eq!(v, U256::from(1e18));

        let v2 = parse_ether_value("1").unwrap();
        assert_eq!(v2, U256::from(1));

        let v3 = parse_ether_value("0.01ether").unwrap();
        assert_eq!(v3, U256::from(1e16));
    }
}