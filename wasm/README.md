```bash
# build wasm
wasm-pack build --release

# patch the wasm file
python3 patch.py
```

## use node zkwasm emulator
```bash
node emulator/index.js <wasm file> <input bin file>
```