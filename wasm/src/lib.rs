use bridge::{VmInput, execute_vm, Artifacts};
use wasm_bindgen::prelude::*;

// mod deserializer;
// use deserializer::from_slice;

extern "C" {
    pub fn wasm_input(is_public: u32) -> u64;
    pub fn wasm_output(v: u64);
    pub fn wasm_read_context() -> u64;
    pub fn wasm_write_context(v: u64);
    pub fn require(cond: bool);
    pub fn wasm_dbg(v: u64);
    pub fn wasm_dbg_char(v: u64);

    pub fn merkle_setroot(x: u64);
    pub fn merkle_address(x: u64);
    pub fn merkle_set(x: u64);
    pub fn merkle_get() -> u64;
    pub fn merkle_getroot() -> u64;
    pub fn merkle_fetch_data() -> u64;
    pub fn merkle_put_data(x: u64);
    pub fn poseidon_new(x: u64);
    pub fn poseidon_push(x: u64);
    pub fn poseidon_finalize() -> u64;

    pub fn babyjubjub_sum_new(x: u64);
    pub fn babyjubjub_sum_push(x: u64);
    pub fn babyjubjub_sum_finalize() -> u64;

}

pub fn wasm_dbg_str(s: &str) {
    unsafe {
        require(s.len() < usize::MAX);
    }
    for i in s.as_bytes() {
        unsafe { wasm_dbg_char(*i as u64) }
    }
}

/* 
fn get_input() -> VmInput {
    // let length = unsafe { wasm_input(1) };
    // let data = wasm_read_u8(length, 0);
    // let words: &[u32] = bytemuck::cast_slice(data.as_slice());
    // from_slice(words).unwrap()
    VmInput {
        header: Default::default(),
        state_trie: Default::default(),
        storage_trie: Default::default(),
        contracts: Default::default(),
        block_hashes: Default::default(),
        poc_contract: Default::default(),
        artifacts: Artifacts {
            storage: Default::default(),
            initial_balance: Default::default(),
        }
    }
}
*/

// fn wasm_read_u8(length: u64, is_public: u32) -> Vec<u8> {
//     let mut i = 0;

//     let mut bytes: Vec<u8> = Vec::with_capacity(length.try_into().unwrap());

//     while i * 8 < length {
//         if i * 8 + 8 < length {
//             unsafe {
//                 let v = wasm_input(is_public);
//                 v.to_le_bytes().iter().for_each(|x| bytes.push(*x))
//             }
//         } else {
//             unsafe {
//                 let v = wasm_input(is_public);
//                 let left = (length - i * 8).try_into().unwrap();
//                 v.to_le_bytes()[0..left].iter().for_each(|x| bytes.push(*x))
//             }
//         }

//         i += 1;              
//     }
//     return bytes;
// }


#[wasm_bindgen]
pub fn zkmain() -> u64 {
    // let vm_input = get_input();
    // execute_vm(vm_input);
    return 0;
}