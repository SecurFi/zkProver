use std::path::PathBuf;
use eyre::{bail, Result};
use revm::primitives::Bytecode;
use foundry_compilers::Project;

pub fn compile_poc(file: impl Into<PathBuf>) -> Result<Bytecode> {
    // let mut selection = Settings::default();
    // selection.push_output_selection(ContractOutputSelection::Metadata);

    let project = Project::builder().offline().ephemeral().no_artifacts().build().unwrap();
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