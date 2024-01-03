use bonsai_sdk::alpha::responses::{SnarkReceipt, Groth16Seal};
use bridge::VmOutput;
use ethers_core::{abi::{Token, Tokenizable}, types::U256};
use eyre::{Result, bail};
use clap::Parser;
use clio::Input;
use zk_guests::{EVM_ID};


#[derive(Parser, Debug)]
pub struct SnarkArgs {
    pub input: Input,
}


impl SnarkArgs {
    pub fn run(self) -> Result<()> {
        let receipt: SnarkReceipt = serde_json::from_reader(self.input)?;
        println!("journal: {}", hex::encode(&receipt.journal));
        println!("post_state_digest: {}", hex::encode(&receipt.post_state_digest));
        let buf = &mut receipt.journal.as_slice();
        let vm_output = VmOutput::decode(buf);
        println!("vm_output: {:?}", vm_output.state_diff);

        let output_tokens = vec![
            Token::Bytes(receipt.journal),
            Token::FixedBytes(receipt.post_state_digest),
            Token::Bytes(ethers_core::abi::encode(&[tokenize_snark_receipt(
                &receipt.snark
            )?])),
        ];
        println!("Image Id: {}", hex::encode(bytemuck::cast::<[u32; 8], [u8; 32]>(EVM_ID)));
        let output = hex::encode(ethers_core::abi::encode(&output_tokens));
        println!("{}", output);
        Ok(())
    }
}

pub fn tokenize_snark_receipt(proof: &Groth16Seal) -> Result<Token> {
    if proof.b.len() != 2 {
        bail!("hex-strings encoded proof is not well formed");
    }
    for pair in [&proof.a, &proof.c].into_iter().chain(proof.b.iter()) {
        if pair.len() != 2 {
            bail!("hex-strings encoded proof is not well formed");
        }
    }
    Ok(Token::FixedArray(vec![
        Token::FixedArray(
            proof
                .a
                .iter()
                .map(|elm| U256::from_big_endian(elm).into_token())
                .collect(),
        ),
        Token::FixedArray(vec![
            Token::FixedArray(
                proof.b[0]
                    .iter()
                    .map(|elm| U256::from_big_endian(elm).into_token())
                    .collect(),
            ),
            Token::FixedArray(
                proof.b[1]
                    .iter()
                    .map(|elm| U256::from_big_endian(elm).into_token())
                    .collect(),
            ),
        ]),
        Token::FixedArray(
            proof
                .c
                .iter()
                .map(|elm| U256::from_big_endian(elm).into_token())
                .collect(),
        ),
    ]))
}
