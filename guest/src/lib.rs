#![cfg_attr(feature = "guest", no_std)]
#![no_main]
use bridge::{VmInput, execute_vm};

#[jolt::provable]
fn simtx(input: VmInput) -> Vec<u8> {
    execute_vm(input).encode()
}