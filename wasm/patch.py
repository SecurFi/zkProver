import re
import subprocess

# download from https://github.com/WebAssembly/wabt/releases
WASM2WAT = "wasm2wat"
WAT2WASM = "wat2wasm"
WASM_PATH = "pkg/zkwasm_prover_bg.wasm"

res = subprocess.run([WASM2WAT, WASM_PATH, "-o", "pkg/origin.wat"])
assert res.returncode == 0

fp = open("pkg/origin.wat", "r")


new_lines = []

inside_func = False
func_depth = 0
clear_func = False
func_lines = []
func_sig = ''

for line in fp.readlines():
    # func start
    if line.strip().startswith("(func"):
        inside_func = True
        func_lines.clear()
        clear_func = 'f64' in line or 'f32' in line
        func_sig = line.strip()
    
    if inside_func:
        clear_func = clear_func or 'f64' in line or 'f32' in line
        func_lines.append(line)
        for c in line:
            if c == '(':
                func_depth += 1
            elif c == ')':
                func_depth -= 1
        if func_depth < 0:
            raise Exception("func depth < 0")
        if func_depth == 0:
            inside_func = False
            if clear_func:
                print("clear func: ", func_sig)
                new_lines.append("(func )\n")
            else:
                new_lines.extend(func_lines)
            
    else:
        new_lines.append(line)

with open("pkg/patched.wat", "w") as fp:
    fp.write(''.join(new_lines))

res = subprocess.run([WAT2WASM, "pkg/patched.wat", "-o", "pkg/zkwasm_prover_bg_patched.wasm"])
assert res.returncode == 0
