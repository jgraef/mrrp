#![allow(dead_code)]

mod config;
mod geo;
mod satellite;
mod satnogs;

use anyhow::{
    Error,
    anyhow,
};
use clap::{
    Parser,
    Subcommand,
};
use directories::ProjectDirs;

use crate::{
    config::Config,
    satellite::Satellites,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::init();

    /*
    https://github.com/csete/gpredict/blob/73937f7e9825747d8482884d25c65904b5ff0ef5/src/sat-cfg.c#L228-L248
    {"TRSP", "SERVER", "https://db.satnogs.org/api/"},
    {"TRSP", "FREQ_FILE", "transmitters/?format=json"},
    {"TRSP", "MODE_FILE", "modes/?format=json"},
    {"TRSP", "PROXY", NULL},
    {"TLE", "SERVER", "https://celestrak.org/NORAD/elements/"},
    {"TLE", "FILES", "amateur.txt;cubesat.txt;visual.txt;weather.txt"},
    {"TLE", "PROXY", NULL},
    {"TLE", "URLS",
     "https://www.amsat.org/amsat/ftp/keps/current/nasabare.txt;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=amateur&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=cubesat&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=galileo&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=glo-ops&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=gps-ops&FORMAT=tlet;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=iridium-NEXT&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=molniya&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=noaa&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=science&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=last-30-days&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=visual&FORMAT=tle;"
     "https://celestrak.org/NORAD/elements/gp.php?GROUP=weather&FORMAT=tle"},
      */

    let args = Args::parse();

    // determine our data directory
    let dirs = ProjectDirs::from("", "switch", "mrrp-sat")
        .ok_or_else(|| anyhow!("Failed to determine project directories"))?;
    let data_dir = dirs.data_dir();

    // load config
    let config_dir = dirs.config_dir();
    if !config_dir.exists() {
        std::fs::create_dir_all(&config_dir)?;
    }
    let config_path = config_dir.join("config.toml");
    if !config_path.exists() {
        tracing::info!(?config_path, "Config not found. Creating default config.");
        let config = Config::default();
        std::fs::write(&config_path, &toml::to_string_pretty(&config)?)?;
        config
    }
    else {
        tracing::info!(?config_path, "Reading config");
        toml::from_slice(&std::fs::read(&config_path)?)?
    };

    // set satkit data directory
    //
    // this is where satkits stores the data necessary for orbit projections. this
    // doesn't contain the satellite TLEs.
    let satkit_data_dir = data_dir.join("satkit_data");
    std::fs::create_dir_all(&satkit_data_dir)?;
    satkit::utils::set_datadir(&satkit_data_dir)?;

    // open satellite data
    let mut satellites = Satellites::open(data_dir.join("satellites"))?;

    match args.command {
        Command::Update => {
            //tracing::info!("Updating satkit data");
            //satkit_update().await?;

            tracing::info!("Updating satellite list");
            satellites.update().await?;
        }
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
    Update,
}

async fn satkit_update() -> Result<(), Error> {
    tokio::task::spawn_blocking(|| satkit::utils::update_datafiles(None, false))
        .await
        .unwrap()
}
