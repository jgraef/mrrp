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
            predict_options,
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
                predict_options.interval,
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
    /// List satellite passes
    ListPasses {
        #[clap(flatten)]
        options: list::Options,

        /// Satellite ID
        ///
        /// This is the SatNOGs satellite ID.
        sat_id: SatelliteId,
    },
    /// Capture baseband when a satellite passes overhead.
    ///
    /// Generated files are 16-bit signed WAV.
    CapturePasses {
        /// Path to directory in which to store captures. Each pass is written
        /// as a single WAV file.
        #[clap(short, long, default_value = ".")]
        output: PathBuf,

        /// The center frequency to capture.
        ///
        /// This does not apply any doppler correction.
        #[clap(short = 'f', long)]
        center_frequency: f32,

        /// The sample rate to capture with.
        #[clap(short, long)]
        sample_rate: f32,

        /// Whether to enable Bias-T.
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
        /// This is the SatNOGs satellite ID.
        sat_id: SatelliteId,
    },
    /// Correct doppler shift
    CorrectDoppler {
        /// Satellite ID
        ///
        /// This is the SatNOGs satellite ID.
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

        #[clap(flatten)]
        predict_options: PredictOptions,

        /// Output file with doppler-corrected signal.
        #[clap(short, long)]
        output: PathBuf,

        /// Input file that is doppler shifted.
        input: PathBuf,
    },
}

#[derive(Clone, Debug, clap::Args)]
pub struct PredictOptions {
    /// How often the orbit propagation should run.
    ///
    /// Orbit propagation is computationally expensive, but doesn't have to
    /// predict the exact satellite state every millsecond. It's usually enough
    /// to calculate the exact state every 5 seconds or so.
    #[clap(short = 'p', long = "predict-interval", default_value = "5s", value_parser = humantime::parse_duration)]
    pub interval: Duration,

    /// How many time steps to calculate at once.
    ///
    /// It's more efficient to calculate many satellite positions in a batch and
    /// buffer them.
    #[clap(short = 'b', long, default_value = "1024")]
    pub batch_size: usize,
}

impl Default for PredictOptions {
    fn default() -> Self {
        Self {
            interval: Duration::from_secs(5),
            batch_size: 1024,
        }
    }
}

#[derive(Clone, Debug, Default, clap::Args)]
pub struct PassOptions {
    #[clap(flatten)]
    pub predict: PredictOptions,

    /// Minimum elevation for a pass to be detected - in degrees.
    #[clap(
        short = 'e',
        long,
        default_value = "0",
        value_parser = crate::util::parse_degrees_as_radians::<f64>,
    )]
    pub min_elevation: f64,
}
