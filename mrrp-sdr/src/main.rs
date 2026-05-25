pub mod build_info;
pub mod cli;
pub mod config;
pub mod directories;
pub mod hal;
pub mod ui;
pub mod util;

use anyhow::Error;
use clap::Parser;
use dotenvy::dotenv;

use crate::{
    cli::{
        Cli,
        Command,
    },
    config::Config,
    directories::Directories,
    ui::run_app,
};

fn main() -> Result<(), Error> {
    let _ = dotenv();
    tracing_subscriber::fmt::init();

    let args = Cli::parse();

    let directories = Directories::new()?;

    let config = Config::read_or_default(directories.config_path())?;

    match args.command.unwrap_or_default() {
        Command::ListRadios => {
            for device in hal::radio::list_devices()? {
                println!("{device:?}");
            }
        }
        Command::Ui(command) => {
            run_app(directories, config, command)?;
        }
    }

    Ok(())
}
