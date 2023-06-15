extern crate core;

use std::{fs, io::Write};

use ethers_solc::{Project, ProjectPathsConfig};

fn main() {
    // Configure the project with all its paths, solc, cache etc.
    let project = Project::builder()
        .paths(ProjectPathsConfig::hardhat(env!("CARGO_MANIFEST_DIR")).unwrap())
        // .offline()
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

    // Tell Cargo that if a source file changes, to rerun this build script.
    project.rerun_if_sources_changed();
}
