mod capture;
mod config;
mod doppler;
mod list;

use std::{
    path::PathBuf,
    time::Duration,
};

use anyhow::{
    Error,
    anyhow,
    bail,
};
use chrono::{
    DateTime,
    Utc,
};
use clap::{
    Parser,
    Subcommand,
};
use mrrp_rtl_sdr::tuner::gain;
use mrrp_sat::{
    Geodetic,
    satellite::{
        Satellite,
        SatelliteDatabase,
        Satellites,
    },
    satnogs::SatelliteId,
    update::Updater,
};

pub use self::config::Config;
use crate::{
    Context,
    commands::rtl_sdr::open::DeviceArgs,
    files::create_parent_dir_if_not_exists,
};

pub async fn run(context: Context<Args>) -> Result<(), Error> {
    let data_dir = context.files.data_dir();
    let config = context.config.sat;

    // open satellite data
    let mut satellites = SatelliteDatabase::open(data_dir.join("satellites.json"))?;

    // perform update if necessary, but skip this invoked with update command
    let updater = Updater::new(&data_dir, config.min_update_interval);
    if !matches!(&context.args.command, Command::Update) && !context.args.no_auto_update {
        updater.perform_auto_update(&mut satellites).await?;
    }

    match context.args.command {
        Command::Update => {
            tracing::info!("Performing forced update");
            updater.perform_update(&mut satellites).await?;
        }
        Command::CorrectDoppler {
            sat_id,
            nominal_frequency,
            start_time,
            file_start_time,
            predict_interval,
            output,
            input,
        } => {
            let start_time = match (start_time, file_start_time) {
                (Some(start_time), false) => start_time,
                (None, true) => {
                    // use file creation time
                    let metadata = std::fs::metadata(&input)?;
                    let created = metadata.created()?;
                    created.into()
                }
                _ => {
                    bail!(
                        "Start time must be specified either via --start-time or --file-start-time."
                    );
                }
            };

            let location = get_location(&config)?;
            let satellite = get_satellite(&satellites, &sat_id)?;

            doppler::correct(
                satellite,
                location,
                nominal_frequency,
                start_time,
                predict_interval,
                &input,
                &output,
            )
            .await?;
        }
        Command::ListPasses {
            sat_id,
            mut options,
        } => {
            if options.end_time.is_none() && options.limit.is_none() {
                options.limit = Some(20);
            }

            let location = get_location(&config)?;
            let satellite = get_satellite(&satellites, &sat_id)?;

            list::list(satellite, location, options, context.global.table_style)?;
        }
        Command::CapturePasses {
            output,
            sat_id,
            center_frequency,
            sample_rate,
            bias_t,
            gain,
            device,
            options,
        } => {
            create_parent_dir_if_not_exists(&output)?;

            let location = get_location(&config)?;
            let satellite = get_satellite(&satellites, &sat_id)?;

            let mut device = device.open_device().await?;
            //let mut device = DummyDevice::default();
            tracing::info!(
                name = device.device_info().name(),
                ?sample_rate,
                ?center_frequency,
                ?bias_t,
                "configuring RTL-SDR"
            );
            device.set_sample_rate(sample_rate).await?;
            device.set_center_frequency(center_frequency).await?;
            device.set_bias_t(bias_t).await?;
            device.set_tuner_gain(gain::Db(gain)).await?;

            capture::capture(
                &output,
                satellite,
                location,
                options,
                &mut device,
                center_frequency,
            )
            .await?;
        }
    }

    Ok(())
}

fn get_location(config: &Config) -> Result<Geodetic, Error> {
    let location = *config
        .location
        .as_ref()
        .ok_or_else(|| anyhow!("Station not configured"))?;

    Ok(location.to_geodetic())
}

fn get_satellite<'a>(
    satellites: &'a Satellites,
    sat_id: &SatelliteId,
) -> Result<&'a Satellite, Error> {
    let satellite = satellites
        .get(
            satellites
                .get_by_id(&sat_id)
                .ok_or_else(|| anyhow!("Satellite not found: {sat_id}"))?,
        )
        .unwrap();

    Ok(satellite)
}

/// Satellite tracking
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,

    #[clap(long)]
    no_auto_update: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Update TLEs
    Update,
    /// Correct doppler shift
    CorrectDoppler {
        /// Satellite ID
        ///
        /// # TODO
        ///
        /// There's currently no good way to find this satellite ID. It's the ID
        /// from the SatNOGS API.
        #[clap(short, long)]
        sat_id: SatelliteId,

        /// The unshifted frequency of the signal.
        #[clap(short = 'f', long)]
        nominal_frequency: f32,

        /// Start time of the capture.
        #[clap(short = 't', long)]
        start_time: Option<DateTime<Utc>>,

        /// Use file creation time as start time.
        #[clap(short = 'T', long)]
        file_start_time: bool,

        /// How often the orbit propagation should run.
        ///
        /// Orbit propagation is computationally expensive, but doesn't have to
        /// predict the exact satellite state for every sample of the input
        /// file. It's usually enough to calculate the exact state every second
        /// or so.
        #[clap(short, long, default_value = "1s", value_parser = humantime::parse_duration)]
        predict_interval: Duration,

        /// Output file with doppler-corrected signal.
        #[clap(short, long)]
        output: PathBuf,

        /// Input file that is doppler shifted.
        input: PathBuf,
    },
    ListPasses {
        #[clap(flatten)]
        options: list::Options,

        /// Satellite ID
        ///
        /// # TODO
        ///
        /// There's currently no good way to find this satellite ID. It's the ID
        /// from the SatNOGS API.
        sat_id: SatelliteId,
    },
    CapturePasses {
        #[clap(short, long, default_value = ".")]
        output: PathBuf,

        /// The center frequency to capture.
        #[clap(short = 'f', long)]
        center_frequency: f32,

        /// The sample rate to capture with.
        #[clap(short, long)]
        sample_rate: f32,

        /// Whether to enable Bias-T
        #[clap(short, long)]
        bias_t: bool,

        /// Tuner gain in dB
        #[clap(short, long)]
        gain: f32,

        #[clap(flatten)]
        device: DeviceArgs,

        #[clap(flatten)]
        options: capture::Options,

        /// Satellite ID
        ///
        /// # TODO
        ///
        /// There's currently no good way to find this satellite ID. It's the ID
        /// from the SatNOGS API.
        sat_id: SatelliteId,
    },
}
