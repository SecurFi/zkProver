# zkProver

Besides generating the proof of exploit using [our online tool](https://www.SecurFi.com/tool), you can also do it locally with zkProver.

## Installation

Currently, zkProver supports **Linux** and **MacOS with Apple Silicon**.

``` bash
curl -L https://install.SecurFi.com | bash
```

> If you're using MacOS, please make sure you have installed Xcode, `xcode-select --install` and run `xcrun metal` successfully.

If you encounter any errors, please contact me via [Telegram](https://t.me/dimitrysc).

## Building from source
### Requirements

* Ubuntu

  - [Rust](https://www.rust-lang.org/tools/install)
   ```bash
   sudo curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```
  - build tools
   ```bash
   sudo apt install -y build-essential pkg-config libssl-dev
   ```
  - cuda[optional] [install guide](https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html)

* Centos
  - Developments Tools
  ```bash
  sudo yum groupinstall 'Development Tools'
  ```
  - cuda[optional] [install guide](https://docs.nvidia.com/cuda/cuda-installation-guide-linux/index.html)

* Mac
  - [Rust](https://www.rust-lang.org/tools/install)
  ```bash
  brew install rustup-init
  rustup-init
  ```

## Usage
### generate zk proof
```bash
# For MacOS with Metal support
cargo run -r -p zkProver -F metal -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>

# For Linux/Windows with CUDA support
cargo run -r -p zkProver -F cuda -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>

# For CPU-only Linux/Windows/MacOS
# Not recommended, the generation might be very slow.
cargo run -r -p zkProver -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>
```

We highly recommend you start hacking from [PoC demos](https://github.com/SecurFi/PoC) after installing zkProver.

## Documentation
[Documentation for SecurFi](https://docs.SecurFi.com)


## Acknowledgements
Thanks to the contributions of [foundry](https://github.com/foundry-rs/foundry), [revm](https://github.com/bluealloy/revm), [reth](https://github.com/paradigmxyz/reth), [ethers-rs](https://github.com/gakonst/ethers-rs) and [RISC Zero](https://github.com/risc0/risc0) to crypto. We're grateful for these awesome projects.
