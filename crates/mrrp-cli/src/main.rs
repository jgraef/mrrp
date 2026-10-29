pub mod commands;
pub mod config;
pub mod files;
pub mod util;

use anyhow::Error;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::{
    commands::Args,
    files::Files,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let files = Files::open()?;
    let args = Args::parse();
    let config = files.config()?;

    commands::run(args, files, config).await?;

    Ok(())
}
