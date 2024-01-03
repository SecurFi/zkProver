#![no_main]

use bridge::{VmInput, execute_vm};
use risc0_zkvm::guest::env;

risc0_zkvm::guest::entry!(main);

pub fn main() {
    let input: VmInput = env::read();
    let output = execute_vm(input);
    // env::commit(&output);
    env::commit_slice(output.encode().as_slice());
    core::mem::forget(output);
}
