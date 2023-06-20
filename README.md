# zkProver

## Requirements
- [Rust](https://www.rust-lang.org/tools/install)

## Usage
### generate zk proof
```bash
# For MacOS with Metal support
cargo run -r -p zkProver -F metal -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>:<contractname>

# For Linux/Windows with CUDA support
cargo run -r -p zkProver -F cuda -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>:<contractname>

# For CPU-only Linux/Windows/MacOS
# Not recommended, the generation might be very slow.
cargo run -r -p zkProver -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>:<contractname>
```

We highly recommend you start hacking from [PoC demos](https://github.com/0xHackedLabs/PoC) after installing zkProver.

## Documentation
[Documentation for 0xHacked](https://docs.0xHacked.com)

## Acknowledgements
Thanks to the contributions of [foundry](https://github.com/foundry-rs/foundry), [revm](https://github.com/bluealloy/revm), [reth](https://github.com/paradigmxyz/reth), [ethers-rs](https://github.com/gakonst/ethers-rs) and [RISC Zero](https://github.com/risc0/risc0) to crypto. We're grateful for these awesome projects.
