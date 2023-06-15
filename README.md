# zkProver

## requirements
- [Rust](https://www.rust-lang.org/tools/install)

## Usage

### generate zk proof
```bash
cargo run -r -p zkProver -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>:<contractname>

# with meta in mac
cargo run -r -p zkProver -F metal -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>:<contractname>

# with cuda in linux/windows
cargo run -r -p zkProver -F cuda -- evm -r <RPC_URL> -b <BLOCK_NUMBER> -d <DEAL> <path>:<contractname>

```

