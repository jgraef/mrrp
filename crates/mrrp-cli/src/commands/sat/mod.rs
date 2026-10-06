#![allow(dead_code)] // todo: remove

mod config;
mod passes;

use std::{
    collections::{
        HashMap,
        HashSet,
    },
    path::{
        Path,
        PathBuf,
    },
    time::Duration,
};

use anyhow::{
    Error,
    anyhow,
    bail,
};
use chrono::{
    DateTime,
    TimeDelta,
    Utc,
};
use clap::{
    Parser,
    Subcommand,
};
use mrrp_audio::{
    WavSource,
    write_stream_to_wav,
};
use mrrp_core::{
    sample::Complex,
    signal::{
        FiniteStream,
        GetSampleRate,
    },
};
use mrrp_sat::{
    Geodetic,
    satellite::{
        ReferenceState,
        Satellite,
        SatelliteDatabase,
        SatelliteHandle,
        Satellites,
    },
    satnogs::SatelliteId,
    tracker::Tracker,
    update::Updater,
};
use mrrp_util::{
    sat::DopplerCorrection,
    signal::AsyncReadSamplesExt,
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
            update_interval: _,
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

            todo!();

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

            correct_doppler(
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

            passes::list(satellite, location, options, context.global.table_style)?;
        }
        Command::CapturePasses {
            output,
            sat_id,
            center_frequency,
            sample_rate,
            device,
        } => {
            create_parent_dir_if_not_exists(&output)?;

            let satellite = get_satellite(&satellites, &sat_id)?;

            // todo
            //let device = device.open_device().await?;
            //let device = ();

            //capture_passes(&output, satellite, device).await?;
            let _ = (center_frequency, sample_rate, device, satellite);

            todo!();
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
    /// List satellites
    List {
        #[clap(flatten)]
        band: BandArgs,

        #[clap(short, long)]
        mode: Vec<String>,
    },
    /// Track satellites
    Track {
        #[clap(flatten)]
        band: BandArgs,

        #[clap(short, long)]
        mode: Vec<String>,

        #[clap(short, long, default_value = "2s", value_parser = humantime::parse_duration)]
        update_interval: Duration,
    },
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
        options: passes::Options,

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

        #[clap(flatten)]
        device: DeviceArgs,

        /// Satellite ID
        ///
        /// # TODO
        ///
        /// There's currently no good way to find this satellite ID. It's the ID
        /// from the SatNOGS API.
        sat_id: SatelliteId,
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

async fn correct_doppler(
    satellite: &Satellite,
    location: Geodetic,
    nominal_frequency: f32,
    start_time: DateTime<Utc>,
    predict_interval: Duration,
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<(), Error> {
    let input = input.as_ref();
    let output = output.as_ref();

    tracing::debug!(sat=%satellite.id(), tle=?satellite.tle(), "Correcting doppler");

    // open source file
    let source = WavSource::<_, Complex<i16>>::from_path(&input)?.convert::<Complex<f32>>();
    let duration = Duration::from_secs_f32(source.len() as f32 / source.sample_rate());

    let doppler_correction = DopplerCorrection::from_satellite(
        satellite,
        ReferenceState::from_geodetic(location),
        nominal_frequency,
        start_time,
        predict_interval,
        source.sample_rate(),
        duration,
    )?;

    // apply correction
    let corrected = source.scan_in_place_with(doppler_correction);

    // write corrected stream to file
    write_stream_to_wav(&output, corrected).await?;

    Ok(())
}
