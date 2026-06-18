// otherwise cargo doc overflow somewhere in wgpu
#![recursion_limit = "256"]

pub mod cli;
pub mod config;
pub mod directories;
pub mod sdr;
pub mod ui;
pub mod util;

use anyhow::Error;
use clap::Parser;
use dotenvy::dotenv;

use crate::{
    cli::Args,
    config::Config,
    directories::Directories,
    ui::run_app,
};

fn main() -> Result<(), Error> {
    let _ = dotenv();
    tracing_subscriber::fmt::init();

    let args = Args::parse();
    let directories = Directories::new()?;
    let config = Config::read_or_default(directories.config_path())?;

    run_app(directories, config, args)?;

    Ok(())
}
