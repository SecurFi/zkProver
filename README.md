# zkProver

## Requirements

#### Ubuntu
- [Rust](https://www.rust-lang.org/tools/install)
```bash
sudo curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```
- build tools
```bash
sudo apt install -y build-essential pkg-config libssl-dev
```
- cuda[optional] [install guide](https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html)

### Centos
- Developments Tools
```bash
sudo yum groupinstall 'Development Tools'
```
- cuda[optional] [install guide](https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html)

#### Mac
- [Rust](https://www.rust-lang.org/tools/install)
```bash
brew install rustup-init
rustup-init
```
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

## todo
1. Maintain an in-house version of [revm](https://github.com/bluealloy/revm), simplify the code, remove unnecessary features, optimize data structures to make it more zk-friendly
2. Multi-chain support
3. Based on Helios, compute asset changes in our zkVerify solution to provide a safe and open-source trustless localRPC node service

## Acknowledgements
Thanks to the contributions of [foundry](https://github.com/foundry-rs/foundry), [revm](https://github.com/bluealloy/revm), [reth](https://github.com/paradigmxyz/reth), [ethers-rs](https://github.com/gakonst/ethers-rs) and [RISC Zero](https://github.com/risc0/risc0) to crypto. We're grateful for these awesome projects.
