use std::{
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
    Local,
    Utc,
};
use clap::{
    Parser,
    Subcommand,
};
use indicatif::{
    ProgressBar,
    ProgressStyle,
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
use mrrp_filter::{
    AsyncReadSamplesFilterExt,
    biquad,
};
use mrrp_geo::Geodetic;
use mrrp_modem::fm::FmDemodulator;
use mrrp_sat::{
    satellite::{
        ReferenceState,
        Satellite,
    },
    satnogs::SatelliteId,
};
use mrrp_util::{
    sat::DopplerCorrection,
    signal::AsyncReadSamplesExt,
};

use crate::{
    Context,
    commands::sat::{
        PredictOptions,
        get_satellite,
    },
    util::{
        sdrpp,
        si,
        stream::Progress,
    },
};

#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Demod {
        #[clap(flatten)]
        demod_options: DemodOptions,

        #[clap(flatten)]
        source_info_options: SourceInfoOptions,

        /// Satellite ID
        ///
        /// This is the SatNOGs satellite ID.
        #[clap(short, long, default_value = "XSKZ-5603-1870-9019-3066")]
        sat_id: SatelliteId,

        #[clap(short, long)]
        output: PathBuf,

        input: PathBuf,
    },
    DemodAll {
        #[clap(flatten)]
        demod_options: DemodOptions,

        /// Satellite ID
        ///
        /// This is the SatNOGs satellite ID.
        #[clap(short, long, default_value = "XSKZ-5603-1870-9019-3066")]
        sat_id: SatelliteId,

        path: PathBuf,
    },
}

#[derive(Debug, clap::Args)]
struct DemodOptions {
    #[clap(short, long, value_parser = si::parse_frequency::<f32>)]
    fm_frequency: f32,

    #[clap(short = 'b', long, value_parser = si::parse_frequency::<f32>)]
    fm_bandwidth: f32,

    #[clap(flatten)]
    predict_options: PredictOptions,
}

#[derive(Debug, clap::Args)]
struct SourceInfoOptions {
    #[clap(short = 'F', long, value_parser = si::parse_frequency::<f32>)]
    center_frequency: Option<f32>,

    #[clap(short = 'S', long)]
    start_time: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Debug)]
struct SourceInfo {
    start_time: DateTime<Utc>,
    frequency: f32,
}

impl SourceInfo {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        let info = sdrpp::parse_path(path)?;

        tracing::debug!(?info, "extracted info from file name");
        if info.recording_type != sdrpp::RecordingType::Baseband {
            bail!("Input file must be baseband");
        }

        Ok(Self {
            start_time: info.timestamp.to_utc(),
            frequency: info.center_frequency as f32,
        })
    }

    pub fn from_path_or_options(
        path: impl AsRef<Path>,
        options: &SourceInfoOptions,
    ) -> Result<Self, Error> {
        Self::from_path(path).or_else(|error| {
            tracing::debug!(%error, "Could not extract file info from file name.");

            let start_time = options
                .start_time
                .ok_or_else(|| anyhow!("Missing start time"))?;
            let frequency = options
                .center_frequency
                .ok_or_else(|| anyhow!("Missing center frequency"))?;

            Ok(Self {
                start_time,
                frequency,
            })
        })
    }

    pub fn with_frequency(mut self, frequency: f32) -> Self {
        self.frequency = frequency;
        self
    }

    pub fn audio_filename(&self) -> String {
        format!(
            "audio_{}Hz_{}.wav",
            self.frequency,
            DateTime::<Local>::from(self.start_time).format("%H-%M-%S_%d-%m-%Y")
        )
    }
}

pub async fn run(args: Args, context: Context) -> Result<(), Error> {
    match args.command {
        Command::Demod {
            demod_options,
            source_info_options,
            sat_id,
            input,
            output,
        } => {
            let source_info = SourceInfo::from_path_or_options(&input, &source_info_options)?;

            let satellites = context.open_satellite_db().await?;
            let location = context.get_geolocation()?;
            let satellite = get_satellite(&satellites, &sat_id)?;

            demod(
                input,
                output,
                satellite,
                location,
                &source_info,
                &demod_options,
            )
            .await?;
        }
        Command::DemodAll {
            demod_options,
            sat_id,
            path,
        } => {
            let satellites = context.open_satellite_db().await?;
            let location = context.get_geolocation()?;
            let satellite = get_satellite(&satellites, &sat_id)?;

            for result in std::fs::read_dir(&path)? {
                let dir_entry = result?;
                let baseband_path = dir_entry.path();
                let Some(baseband_filename) = baseband_path
                    .file_name()
                    .and_then(|file_name| file_name.to_str())
                else {
                    continue;
                };
                let baseband_metadata = dir_entry.metadata()?;

                if baseband_filename.starts_with("baseband") {
                    let source_info = SourceInfo::from_path(&baseband_path)?;
                    tracing::debug!(filename = ?baseband_filename, ?source_info, "processing baseband");

                    let audio_filename = source_info
                        .with_frequency(demod_options.fm_frequency)
                        .audio_filename();
                    let audio_path = path.join(&audio_filename);

                    if let Ok(audio_metadata) = audio_path.metadata() {
                        tracing::debug!(filename = ?audio_filename, "audio file already exists");

                        if audio_metadata.modified()? < baseband_metadata.modified()? {
                            tracing::debug!("baseband is newer than audio. replacing.");
                        }
                        else {
                            tracing::debug!("baseband is older than audio. skipping.");
                            continue;
                        }
                    }
                    else {
                        tracing::debug!(filename = ?audio_filename, "creating audio file");
                    }

                    let _ = (location, satellite, &demod_options);
                    demod(
                        &baseband_path,
                        &audio_path,
                        satellite,
                        location,
                        &source_info,
                        &demod_options,
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}

async fn demod(
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    satellite: &Satellite,
    location: Geodetic,
    source_info: &SourceInfo,
    demod_options: &DemodOptions,
) -> Result<(), Error> {
    let input = input.as_ref();
    let output = output.as_ref();

    // open source file
    let source = WavSource::<_, Complex<i16>>::from_path(&input)?.convert::<Complex<f32>>();
    let sample_rate = source.sample_rate();
    let num_samples = source.len();
    let duration = Duration::from_secs_f32(num_samples as f32 / sample_rate);

    tracing::debug!(sat=%satellite.id(), start_time = ?source_info.start_time, center_frequency = ?source_info.frequency, ?duration, "Decoding ISS SSTV");

    if demod_options.fm_frequency - 0.5 * demod_options.fm_bandwidth
        < source_info.frequency - 0.5 * sample_rate
        || demod_options.fm_frequency + 0.5 * demod_options.fm_bandwidth
            > source_info.frequency + 0.5 * sample_rate
    {
        bail!(
            "FM frequency ({}) with FM bandwidth ({}) is out of range [{}, {}]",
            si::format(demod_options.fm_frequency, "Hz"),
            si::format(demod_options.fm_bandwidth, "Hz"),
            si::format(source_info.frequency - 0.5 * sample_rate, "Hz"),
            si::format(source_info.frequency + 0.5 * sample_rate, "Hz")
        )
    }

    // track progress
    let progress_bar = ProgressBar::new(num_samples as u64);
    progress_bar.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} {percent} % [{elapsed_precise}, eta: {eta}] [{wide_bar:.cyan/blue}] ",
        )
        .unwrap(),
    );
    let source = Progress::new(source, |num_samples: usize| {
        progress_bar.inc(num_samples as u64);
    });

    // doppler correction
    let mut doppler_correction = DopplerCorrection::from_satellite(
        satellite,
        ReferenceState::from_geodetic(location),
        demod_options.fm_frequency,
        source_info.start_time,
        demod_options.predict_options.interval,
        source.sample_rate(),
        duration,
    )?;

    // apply constant frequency shift to correction to shift FM frequency to DC
    doppler_correction.fixed_offset = source_info.frequency - demod_options.fm_frequency;

    // signal is shifted to DC - taking doppler shift into account
    let signal = source.scan_in_place_with(doppler_correction);

    // lowpass filter
    let lowpass = biquad::lowpass(sample_rate, 0.5 * demod_options.fm_bandwidth);
    let signal = signal.scan_in_place_with(lowpass);

    // decimate to ~ FM bandwidth
    let signal = signal.decimate_to(demod_options.fm_bandwidth);
    let sample_rate = signal.sample_rate();

    // FM demodulate
    let signal = signal.scan_with(FmDemodulator::new(
        sample_rate,
        0.5 * demod_options.fm_bandwidth,
    ));

    // write FM demodulated audio to file
    tracing::info!("Demodulating...");

    tracing::info!(
        sample_rate = %si::format(signal.sample_rate(), "Hz"),
        "output"
    );

    tracing::info!(?output, "Writing demodulated audio to file");
    write_stream_to_wav(&output, signal).await?;

    Ok(())
}
