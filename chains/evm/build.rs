extern crate core;

use std::{fs, io::Write, env, path::Path};

use foundry_compilers::{Project, ProjectPathsConfig};
use alloy_json_abi::ContractObject;


fn main() {
    // Configure the project with all its paths, solc, cache etc.
    let project = Project::builder()
        .paths(ProjectPathsConfig::hardhat(env!("CARGO_MANIFEST_DIR")).unwrap())
        .offline()
        .build()
        .unwrap();
    let output = project.compile().unwrap();

    if output.has_compiler_errors() || output.has_compiler_warnings() {
        let mut tty = fs::OpenOptions::new().write(true).open("/dev/tty").ok();

        if let Some(tty) = &mut tty {
            for error in output.clone().output().errors.iter() {
                write!(tty, "{}", error).unwrap();
            }
            if output.has_compiler_errors() {
                panic!("Failed to build Solidity contracts");
            }
        } else {
            panic!("{:?}", output.output().errors);
        }
    }

    let path = "artifacts/Deal.sol/Deal.json";
    let json = std::fs::read_to_string(path).unwrap();
    let contract: ContractObject = serde_json::from_str(&json).unwrap();
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("deal_contract.rs");
    
    let code = contract.deployed_bytecode.unwrap();
    let code_u8 = code.as_ref();
    let content = format!(
        r##"
        pub const DEAL_CONTRACT_CODE: &[u8] = &{code_u8:?};
        "##
    );
    fs::write(&dest_path, content).unwrap();
    // Tell Cargo that if a source file changes, to rerun this build script.
    project.rerun_if_sources_changed();
}
