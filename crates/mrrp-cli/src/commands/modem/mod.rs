pub mod adsb;
pub mod sstv;

use anyhow::Error;
use clap::{
    Parser,
    Subcommand,
};

use crate::Context;

pub async fn run(args: Args, context: Context) -> Result<(), Error> {
    match args.command {
        Command::Sstv(args) => {
            sstv::run(args, context).await?;
        }
        Command::Adsb(args) => {
            adsb::run(args, context).await?;
        }
    }

    Ok(())
}

#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    Sstv(sstv::Args),
    Adsb(adsb::Args),
}
