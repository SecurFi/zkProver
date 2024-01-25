use ethers::{
    solc::{
        Project, info::ContractInfo, ConfigurableContractArtifact,
        SolcConfig, artifacts::{output_selection::ContractOutputSelection, Settings}, ConfigurableArtifacts, Solc
    },
};



pub fn compile_contract(contract: &ContractInfo) -> eyre::Result<ConfigurableContractArtifact> {
    let artifacts = ConfigurableArtifacts::new(vec![ContractOutputSelection::Metadata], vec![]);
    let settings = Settings::default().with_extra_output(vec![ContractOutputSelection::Metadata]);
    let solc_config = SolcConfig::builder()
        .settings(settings)
        .build();
    
    let solc = Solc::find_or_install_svm_version("0.8.19").expect("could not install solc");
    let project = Project::builder()
        .artifacts(artifacts)
        .set_cached(false)
        .set_no_artifacts(true)
        .offline()
        .solc(solc)
        .solc_config(solc_config)
        .build().unwrap();
    let mut output = project.compile_files(vec![contract.path.clone().unwrap()]).unwrap();
    if output.has_compiler_errors() {
        eyre::bail!("Failed to build Solidity contracts")
    }

    let contract = if let Some(contract) = output.remove_contract(contract) {
        contract
    } else {
        let err = format!("count not find contract {}", contract.name);
        eyre::bail!(err)
    };


    Ok(contract)
}
