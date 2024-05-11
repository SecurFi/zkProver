#![allow(non_snake_case)]

use std::future::Future;
use clap::{Parser, Subcommand};
use anyhow::Result;
mod chains;
use chains::evm::EvmArgs;
mod proof;
mod tools;
use tools::{PackArgs, PreArgs};
mod verify;
use verify::VerifyArgs;


#[derive(Debug, Parser)]
#[clap(author, version, about, long_about=None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run the EVM proof generator
    Evm(EvmArgs),
    Pre(PreArgs),
    Pack(PackArgs),
    Verify(VerifyArgs),
}

#[allow(unused)]
pub fn block_on<F: Future>(future: F) -> F::Output {
    let rt = tokio::runtime::Runtime::new().expect("could not start tokio rt");
    rt.block_on(future)
}



fn main() -> Result<()> {
    env_logger::init();

    let args = Cli::parse();
    match args.command {
        Commands::Evm(args) => block_on(args.run()),
        Commands::Pre(args) => block_on(args.run()),
        Commands::Pack(args) => args.run(),
        Commands::Verify(args) => block_on(args.run())
    }
}
