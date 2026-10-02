pub mod commands;
pub mod config;
pub mod files;

use anyhow::Error;
use clap::{
    Parser,
    Subcommand,
};

use crate::files::Files;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::init();

    let files = Files::open()?;
    let args = Args::parse();

    match args.command {
        Command::RtlSdr(args) => commands::rtl_sdr::run(args).await?,
        Command::Sat(args) => commands::sat::run(args, files).await?,
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    RtlSdr(commands::rtl_sdr::Args),
    Sat(commands::sat::Args),
}
