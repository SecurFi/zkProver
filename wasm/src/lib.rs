use bridge::{VmInput, execute_vm};
use wasm_bindgen::prelude::*;

mod deserializer;
use deserializer::from_slice;

extern "C" {
    pub fn wasm_input(is_public: u32) -> u64;
    pub fn wasm_output(v: u64);
}


fn get_input() -> VmInput {
    let length = unsafe { wasm_input(1) };
    let data = wasm_read_u8(length, 0);
    let words: &[u32] = bytemuck::cast_slice(data.as_slice());
    let input: VmInput = from_slice(words).unwrap();
    return input;
}


fn wasm_read_u8(length: u64, is_public: u32) -> Vec<u8> {
    let mut i = 0;

    let mut bytes: Vec<u8> = Vec::with_capacity(length.try_into().unwrap());

    while i * 8 < length {
        if i * 8 + 8 < length {
            unsafe {
                let v = wasm_input(is_public);
                v.to_le_bytes().iter().for_each(|x| bytes.push(*x))
            }
        } else {
            unsafe {
                let v = wasm_input(is_public);
                let left = (length - i * 8).try_into().unwrap();
                v.to_le_bytes()[0..left].iter().for_each(|x| bytes.push(*x))
            }
        }

        i += 1;              
    }
    return bytes;
}


#[wasm_bindgen]
pub fn zkmain(){
    let vm_input = get_input();
    let output = execute_vm(vm_input);
    output.encode().as_slice().chunks(8).into_iter().for_each(|x| {
        let mut data = [0u8; 8];
        data[..x.len()].copy_from_slice(x);
        unsafe {
            wasm_output(u64::from_le_bytes(data));
        }        
    })
}