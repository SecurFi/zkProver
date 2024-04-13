#![allow(non_snake_case)]

use std::future::Future;
use std::error::Error;
use clap::{Parser, Subcommand};
use eyre::EyreHandler;
mod chains;
use chains::evm::EvmArgs;


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
}

#[allow(unused)]
pub fn block_on<F: Future>(future: F) -> F::Output {
    let rt = tokio::runtime::Runtime::new().expect("could not start tokio rt");
    rt.block_on(future)
}


#[derive(Debug)]
pub struct Handler;

impl EyreHandler for Handler {
    fn debug(
        &self,
        error: &(dyn Error + 'static),
        f: &mut core::fmt::Formatter<'_>,
    ) -> core::fmt::Result {
        if f.alternate() {
            return core::fmt::Debug::fmt(error, f)
        }
        writeln!(f)?;
        write!(f, "{}", error)?;

        if let Some(cause) = error.source() {
            write!(f, "\n\nContext:")?;

            let multiple = cause.source().is_some();
            let errors = std::iter::successors(Some(cause), |e| (*e).source());

            for (n, error) in errors.enumerate() {
                writeln!(f)?;
                if multiple {
                    write!(f, "- Error #{n}: {error}")?;
                } else {
                    write!(f, "- {error}")?;
                }
            }
        }

        Ok(())
    }
}


fn main() -> eyre::Result<()> {
    let mut builder = env_logger::Builder::from_default_env();
    builder.try_init()?;
    eyre::set_hook(Box::new(move |_| Box::new(Handler)))?;
    
    let args = Cli::parse();
    match args.command {
        Commands::Evm(args) => block_on(args.run()),
    }
}
