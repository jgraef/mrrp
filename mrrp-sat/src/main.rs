#![allow(dead_code)]

mod config;
mod geo;
mod satellite;
mod satnogs;
mod track;
mod update;

use std::{
    collections::{
        HashMap,
        HashSet,
    },
    time::Duration,
};

use anyhow::{
    Error,
    anyhow,
};
use chrono::TimeDelta;
use clap::{
    Parser,
    Subcommand,
};
use directories::ProjectDirs;

use crate::{
    config::Config,
    satellite::{
        SatelliteDatabase,
        SatelliteHandle,
        Satellites,
    },
    track::Tracker,
    update::Updater,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();

    tracing_subscriber::fmt::init();

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
    let config = if !config_path.exists() {
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
    let mut satellites = SatelliteDatabase::open(data_dir.join("satellites.json"))?;

    // perform update if necessary, but skip this invoked with update command
    let updater = Updater::new(&data_dir);
    if !matches!(&args.command, Command::Update) && !args.no_auto_update {
        updater.perform_auto_update(&mut satellites).await?;
    }

    match args.command {
        Command::Update => {
            tracing::info!("Performing forced update");
            updater.perform_update(&mut satellites).await?;
        }
        Command::List { band, mode } => {
            let track_set = track_set(&satellites, &band, &mode);

            for (&handle, transmitters) in track_set.iter() {
                let satellite = satellites.get(handle).unwrap();
                print!("{}:", satellite.name());
                for &transmitter in transmitters {
                    let transmitter = &satellite.transmitters()[transmitter];
                    let frequency = transmitter
                        .downlink_low
                        .or(transmitter.downlink_high)
                        .unwrap();
                    print!(" {:.3}", frequency as f32 / 1000000.0);
                }
                println!("");
            }
        }
        Command::Track {
            band,
            mode,
            update_interval,
        } => {
            let track_set = track_set(&satellites, &band, &mode);

            let mut tracker = Tracker::new_with_file_backed_cache(
                config.tracker_options(),
                data_dir.join("tracker_cache.json"),
                &satellites,
            )?;

            for (&handle, _) in &track_set {
                let satellite = satellites.get(handle).unwrap();
                tracing::debug!(?satellite, "tracking");

                tracker.track(handle);
            }

            tracker.update(&mut satellites);
            tracker.flush_cache_to_file(&satellites)?;

            /*let mut update_interval = tokio::time::interval(update_interval);

            abort_on_ctrl_c(async move {
                loop {
                    update_interval.tick().await;

                    tracker.update(&mut satellites);
                    tracker.flush_cache_to_file(&satellites)?;
                }
            })
            .await?;*/
        }
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(subcommand)]
    command: Command,

    #[clap(long)]
    no_auto_update: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    Update,
    List {
        #[clap(flatten)]
        band: BandArgs,

        #[clap(short, long)]
        mode: Vec<String>,
    },
    Track {
        #[clap(flatten)]
        band: BandArgs,

        #[clap(short, long)]
        mode: Vec<String>,

        #[clap(short, long, default_value = "2s", value_parser = humantime::parse_duration)]
        update_interval: Duration,
    },
}

/*
#[derive(Debug, clap::Args)]
struct TimeSpanArgs {
    #[clap(long)]
    start_time: Option<DateTime<Utc>>,

    #[clap(long)]
    end_time: Option<DateTime<Utc>>,

    #[clap(long, value_parser = parse_arg_duration)]
    duration: Option<Duration>,
}

impl TimeSpanArgs {
    fn canonicalize(&self) -> Result<(DateTime<Utc>, DateTime<Utc>), Error> {
        let (start, end) = match (self.start_time, self.end_time, self.duration) {
            (Some(_), Some(_), Some(_)) => {
                bail!("When specifying --duration, only one of --start or --end can be used.")
            }
            (Some(start), Some(end), None) => (start, end),
            (Some(start), None, Some(duration)) => (start, start + duration),
            (None, Some(end), Some(duration)) => (end - duration, end),
            (None, None, Some(duration)) => {
                let start = Utc::now();
                (start, start + duration)
            }
            (Some(start), None, None) => {
                let duration = TimeDelta::hours(2);
                (start, start + duration)
            }
            (None, Some(end), None) => {
                let duration = TimeDelta::hours(2);
                (end - duration, end)
            }
            (None, None, None) => {
                let start = Utc::now();
                let duration = TimeDelta::hours(2);
                (start, start + duration)
            }
        };
        Ok((start, end))
    }
} */

#[derive(Debug, clap::Args)]
struct BandArgs {
    #[clap(long)]
    start_frequency: Option<u64>,

    #[clap(long)]
    end_frequency: Option<u64>,
}

fn parse_arg_duration(s: &str) -> Result<TimeDelta, Error> {
    Ok(TimeDelta::from_std(humantime::parse_duration(s)?)?)
}

fn track_set(
    satellites: &Satellites,
    band: &BandArgs,
    modes: &[impl ToString],
) -> HashMap<SatelliteHandle, Vec<usize>> {
    let modes = modes
        .into_iter()
        .map(|s| s.to_string())
        .collect::<HashSet<String>>();

    satellites
        .iter()
        .filter_map(|satellite| {
            let matched_transmitters = satellite
                .transmitters()
                .iter()
                .enumerate()
                .filter_map(|(index, transmitter)| {
                    let mode_matched = transmitter
                        .mode
                        .as_ref()
                        .is_none_or(|mode| modes.contains(&mode.0));

                    let band_matched =
                        if band.start_frequency.is_none() && band.end_frequency.is_none() {
                            true
                        }
                        else if transmitter.downlink_low.is_none()
                            && transmitter.downlink_high.is_none()
                        {
                            false
                        }
                        else {
                            let matches = |f| {
                                band.start_frequency
                                    .is_none_or(|filter_low| f >= filter_low)
                                    && band
                                        .end_frequency
                                        .is_none_or(|filter_high| f <= filter_high)
                            };

                            transmitter.downlink_low.is_none_or(matches)
                                && transmitter.downlink_high.is_none_or(matches)
                        };

                    if mode_matched && band_matched {
                        Some(index)
                    }
                    else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if satellite.is_alive() && !matched_transmitters.is_empty() {
                Some((satellite.handle(), matched_transmitters))
            }
            else {
                None
            }
        })
        .collect::<HashMap<SatelliteHandle, Vec<usize>>>()
}

async fn abort_on_ctrl_c(f: impl Future<Output = Result<(), Error>>) -> Result<(), Error> {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => Ok(()),
        ret = f => ret,
    }
}
