use clap::{Parser, Subcommand};
use std::future::Future;
mod chains;
use chains::evm::EvmArgs;
mod proof;
pub const VERSION_MESSAGE: &str = "0.1.0";

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about=None, version = VERSION_MESSAGE)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run the EVM proof generator
    Evm(EvmArgs),
}

#[allow(unused)]
pub fn block_on<F: Future>(future: F) -> F::Output {
    let rt = tokio::runtime::Runtime::new().expect("could not start tokio rt");
    rt.block_on(future)
}

fn main() -> eyre::Result<()> {
    let args = Cli::parse();
    match args.command {
        Commands::Evm(args) => block_on(args.run()),
    }
}
