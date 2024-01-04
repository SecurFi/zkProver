use std::path::PathBuf;
use eyre::{bail, Result};
use revm::primitives::Bytecode;
use foundry_compilers::{Project, Solc, artifacts::Settings, EvmVersion, SolcConfig};

pub fn compile_poc(file: impl Into<PathBuf>) -> Result<Bytecode> {
    let mut settings = Settings::default();
    settings.evm_version = Some(EvmVersion::Shanghai);
    let solc_config = SolcConfig { settings: settings };
    let solc = Solc::find_or_install_svm_version("0.8.19").expect("could not install solc");
    let project = Project::builder().solc(solc).solc_config(solc_config).offline().ephemeral().no_artifacts().build().unwrap();
    let mut output = project.compile_files(vec![file, ]).unwrap();
    if output.has_compiler_errors() {
        bail!("Faield to build Solidity contracts")
    }
    
    let contract = output.remove_first("Exploit");
    if contract.is_none() {
        bail!("Can not find 'Exploit' contract")
    }
    Ok(Bytecode::new_raw(contract.unwrap().deployed_bytecode.unwrap().bytecode.unwrap().object.into_bytes().unwrap()))
}