#![allow(dead_code)] // todo: remove

mod config;

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
    geo::Geodetic,
    satellite::{
        OrbitPropagationCache,
        ReferenceState,
        SatelliteDatabase,
        SatelliteHandle,
        SatelliteState,
        Satellites,
    },
    satnogs::SatelliteId,
    tracker::Tracker,
    update::Updater,
};
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    ComplexSinusoid,
    SignalGenerator,
};

pub use self::config::Config;
use crate::files::Files;

pub async fn run(args: Args, files: Files) -> Result<(), Error> {
    let config = files.config()?.sat;
    let data_dir = files.data_dir();

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
            predict_interval,
            output,
            input,
        } => {
            let start_time = start_time.map(Ok).unwrap_or_else(|| {
                // use file creation time
                let metadata = std::fs::metadata(&input)?;
                let created = metadata.created()?;
                Ok::<DateTime<Utc>, Error>(created.into())
            })?;

            let location = config
                .station
                .as_ref()
                .ok_or_else(|| anyhow!("Station not configured"))?
                .location;

            correct_doppler(
                &mut satellites,
                &sat_id,
                location,
                nominal_frequency,
                start_time,
                predict_interval,
                &input,
                &output,
            )
            .await?;
        }
    }

    Ok(())
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

        /// The unshifted frequency of the signal
        #[clap(short = 'f', long)]
        nominal_frequency: f32,

        /// Start time of the capture.
        ///
        /// If omitted the file creation time will be used.
        #[clap(short = 't', long)]
        start_time: Option<DateTime<Utc>>,

        /// How often the orbit propagation should run.
        ///
        /// Orbit propagation is computationally expensive, but doesn't have to
        /// predict the exact satellite state for every sample of the input
        /// file. It's usually enough to calculate the exact state every second
        /// or so.
        #[clap(short, long, default_value = "1s", value_parser = humantime::parse_duration)]
        predict_interval: Duration,

        /// Output file with doppler-corrected signal
        #[clap(short, long)]
        output: PathBuf,

        /// Input file that is doppler shifted
        input: PathBuf,
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

async fn correct_doppler(
    satellites: &mut Satellites,
    sat_id: &SatelliteId,
    location: Geodetic,
    nominal_frequency: f32,
    start_time: DateTime<Utc>,
    predict_interval: Duration,
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<(), Error> {
    let input = input.as_ref();
    let output = output.as_ref();

    tracing::debug!(%sat_id, nominal_frequency, ?input, ?output, "Correcting doppler");

    // get satellite (TLEs)
    let satellite_handle = satellites
        .get_by_id(sat_id)
        .ok_or_else(|| anyhow!("Satellite not found: {sat_id}"))?;
    let satellite = satellites.get(satellite_handle).unwrap();
    tracing::debug!(sat=satellite.name(), tle=?satellite.tle(), "Satellite");

    // open source file
    let source = WavSource::<_, Complex<i16>>::from_path(&input)?.convert::<Complex<f32>>();
    let source_length = source.len();
    let source_duration = Duration::from_secs_f32(source_length as f32 / source.sample_rate());

    // calculate timing information
    let num_intervals =
        (source_duration.as_secs_f32() / predict_interval.as_secs_f32()).ceil() as usize;
    let num_samples_per_interval =
        (predict_interval.as_secs_f32() * source.sample_rate()).floor() as usize;
    let effective_predict_interval = num_samples_per_interval as f32 * source.sample_rate();

    // calculate exact timestamps for which we want satellite states
    let times = (0..num_intervals)
        .map(|i| start_time + Duration::from_secs_f32(effective_predict_interval * i as f32))
        .collect::<Vec<_>>();

    // predict satellite states for the time spanned by the source
    let mut orbit_propagation_cache = OrbitPropagationCache::default();
    let satellite_states = satellite.predict_state(&times, &mut orbit_propagation_cache)?;

    // create mixing factor to correct dopller shift
    let mut correction = DopplerShiftCorrection::new(
        satellite_states,
        ReferenceState::from_geodetic(location),
        nominal_frequency,
        source.sample_rate(),
    );

    // apply correction
    let corrected = source.map_in_place(|sample| sample * correction.next());

    // write corrected stream to file
    write_stream_to_wav(&output, corrected).await?;

    Ok(())
}

#[derive(Clone, Debug)]
pub struct DopplerShiftCorrection {
    satellite_states: Vec<SatelliteState>,
    reference_state: ReferenceState,
    nominal_frequency: f32,
    sinusoid: ComplexSinusoid,
    start_time: DateTime<Utc>,
    sample_index: usize,
    state_index: usize,
}

impl DopplerShiftCorrection {
    pub fn new(
        satellite_states: Vec<SatelliteState>,
        reference_state: ReferenceState,
        nominal_frequency: f32,
        sample_rate: f32,
    ) -> Self {
        // note: we initialize the frequency to 1 Hz, but it will be updated
        // before generating the first sample
        let sinusoid = ComplexSinusoid::new(1.0, sample_rate);

        let start_time = satellite_states
            .get(0)
            .unwrap_or_else(|| panic!("satellite_states must not be empty."))
            .time();

        Self {
            satellite_states,
            reference_state,
            nominal_frequency,
            sinusoid,
            start_time,
            sample_index: 0,
            state_index: 0,
        }
    }
}

impl SignalGenerator for DopplerShiftCorrection {
    type Sample = Complex<f32>;

    fn next(&mut self) -> Self::Sample {
        // or should we just calculate the delta time per sample ahead of time
        // and use that? our concern was that this would accumulate error.
        let time = self.start_time
            + Duration::from_secs_f32(self.sample_index as f32 * self.sinusoid.sample_rate());

        // current state we're at
        let mut current_state = &self.satellite_states[self.state_index];

        // check if we moved on to the next state
        if let Some(next_state) = self.satellite_states.get(self.state_index + 1) {
            if time >= next_state.time() {
                self.state_index += 1;
                current_state = next_state;
            }
        }

        let relative_state = current_state.relative(&self.reference_state);
        let doppler_shift = relative_state.doppler_shift(self.nominal_frequency as f64) as f32;

        self.sample_index += 1;

        self.sinusoid.set_frequency(-doppler_shift);
        self.sinusoid.next()
    }
}

impl GetSampleRate for DopplerShiftCorrection {
    fn sample_rate(&self) -> f32 {
        self.sinusoid.sample_rate()
    }
}
