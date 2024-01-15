import { readFile } from 'node:fs/promises';
import { WASI } from 'wasi';
import { argv, env } from 'node:process';
import fs from "fs"



function bigintToBigEndianHexString(bigint) {
    const buffer = Buffer.alloc(8);
    buffer.writeBigInt64BE(bigint);
    return buffer.toString('hex');
}


(async function () {
  const wasi = new WASI({
    version: 'preview1',
    args: argv,
    env,
    returnOnExit: true
  });
  console.log("argv: ", process.argv);
  const wasm = await WebAssembly.compile(
    await readFile(process.argv[2]),
  );

  let instance
  let cur = 0
  let inputSize = BigInt(process.argv[3])
  let preimages = fs.readFileSync(process.argv[4])
  console.log("file size: ", preimages.length);
  let first_output = true;
  const hostio = {
    env: {
      wasm_input: (ispulic) => {
        if (ispulic) {
          return inputSize;
        }
        let data = preimages.readBigInt64BE(cur)
        cur += 8
        return data
      },
      require: (cond) => {
        if (cond == 0) {
          console.log("require is not satisfied");
          process.exit(1);
        }
      },
      wasm_output:(value) => {
        if (first_output) {
            console.log("wasm_output:")
            first_output = false
        }
        process.stdout.write(bigintToBigEndianHexString(value))
      },
      wasm_dbg_char: (value) => {
        process.stdout.write(String.fromCharCode(Number(value)))
        // console.error(value)
      }
    },
  }

  instance = await WebAssembly.instantiate(wasm, hostio);
  wasi.start(instance);
})()