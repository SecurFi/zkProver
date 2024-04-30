use std::path::PathBuf;

use eyre::{Result, bail};
use ethers_solc::{
    Project, ConfigurableContractArtifact,
    SolcConfig, artifacts::Settings, 
    Solc, EvmVersion
};



pub fn compile_contract(file: impl Into<PathBuf>) -> Result<ConfigurableContractArtifact> {
    let mut settings = Settings::default();
    settings.evm_version = Some(EvmVersion::Shanghai);
    let solc_config = SolcConfig { settings: settings };

    let solc = Solc::find_or_install_svm_version("0.8.20").expect("could not install solc");
    let project = Project::builder().solc(solc).solc_config(solc_config).offline().ephemeral().no_artifacts().build().unwrap();

    let mut output = project.compile_files(vec![file, ]).unwrap();
    if output.has_compiler_errors() {
        bail!("Failed to build Solidity contracts")
    }

    let contract = output.remove_first("Exploit");
    if contract.is_none() {
        bail!("Can not find 'Exploit' contract")
    }


    Ok(contract.unwrap())
}
