pub mod sstv;

use std::{
    fs::File,
    io::{
        BufReader,
        Read,
        Seek,
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
use mrrp_file::riff::{
    self,
    ChunkId,
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
        sdrpp::{
            self,
            RecordingType,
        },
        si,
        stream::Progress,
    },
};

pub async fn run(args: Args, context: Context) -> Result<(), Error> {
    match args.command {
        Command::ReadRiff { path } => {
            fn print_chunk_info<R>(
                mut reader: &mut riff::Reader<R>,
                chunk_ref: riff::ChunkRef,
                level: usize,
            ) -> Result<(), Error>
            where
                R: Read + Seek,
            {
                for _ in 0..level {
                    print!(" ");
                }
                println!("- {chunk_ref:?}");

                if chunk_ref.id() == ChunkId::RIFF || chunk_ref.id() == ChunkId::LIST {
                    let mut children = vec![];
                    let mut list = reader.list(chunk_ref)?;
                    while let Some(child_ref) = list.next()? {
                        children.push(child_ref);
                    }

                    for child_ref in children {
                        print_chunk_info(&mut reader, child_ref, level)?;
                    }
                }

                Ok(())
            }

            let mut reader = riff::Reader::new(BufReader::new(File::open(&path)?));
            let chunk_ref = reader.riff()?;
            print_chunk_info(&mut reader, chunk_ref, 0)?;
        }
        Command::IssDemod {
            center_frequency,
            start_time,
            sat_id,
            fm_frequency,
            fm_bandwidth,
            predict_options,
            input,
            output,
        } => {
            let satellites = context.open_satellite_db().await?;

            let location = context.get_geolocation()?;
            let satellite = get_satellite(&satellites, &sat_id)?;

            iss_demod(
                center_frequency,
                start_time,
                input,
                output,
                satellite,
                location,
                fm_frequency,
                fm_bandwidth,
                predict_options,
            )
            .await?;
        }
        Command::Sstv(args) => {
            sstv::run(
                args,
                Context {
                    global: context.global,
                    files: context.files,
                    config: context.config,
                },
            )
            .await?;
        }
    }

    Ok(())
}

/// Test commands
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print chunks contained in a RIFF file.
    ReadRiff {
        path: PathBuf,
    },
    IssDemod {
        #[clap(short = 'F', long, value_parser = si::parse_frequency::<f32>)]
        center_frequency: Option<f32>,

        #[clap(short = 'S', long)]
        start_time: Option<DateTime<Utc>>,

        /// Satellite ID
        ///
        /// This is the SatNOGs satellite ID.
        #[clap(short, long, default_value = "XSKZ-5603-1870-9019-3066")]
        sat_id: SatelliteId,

        #[clap(short, long, value_parser = si::parse_frequency::<f32>)]
        fm_frequency: f32,

        #[clap(short = 'b', long, value_parser = si::parse_frequency::<f32>)]
        fm_bandwidth: f32,

        #[clap(flatten)]
        predict_options: PredictOptions,

        #[clap(short, long)]
        output: PathBuf,

        input: PathBuf,
    },
    Sstv(sstv::Args),
}

async fn iss_demod(
    center_frequency: Option<f32>,
    start_time: Option<DateTime<Utc>>,
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
    satellite: &Satellite,
    location: Geodetic,
    fm_frequency: f32,
    fm_bandwidth: f32,
    predict_options: PredictOptions,
) -> Result<(), Error> {
    // extract stream info from file name
    let (center_frequency, start_time) = match sdrpp::parse_path(&input) {
        Ok(info) => {
            tracing::debug!(?info, "extracted info from file name");
            if info.recording_type != RecordingType::Baseband {
                bail!("Input file must be baseband");
            }

            (info.center_frequency as f32, info.timestamp.to_utc())
        }
        Err(error) => {
            tracing::debug!(%error, "Could not extract file info from file name.");

            let center_frequency =
                center_frequency.ok_or_else(|| anyhow!("Missing center frequency"))?;
            let start_time = start_time.ok_or_else(|| anyhow!("Missing start time"))?;

            (center_frequency, start_time)
        }
    };

    // open source file
    let source = WavSource::<_, Complex<i16>>::from_path(&input)?.convert::<Complex<f32>>();
    let sample_rate = source.sample_rate();
    let num_samples = source.len();
    let duration = Duration::from_secs_f32(num_samples as f32 / sample_rate);

    tracing::debug!(sat=%satellite.id(), ?center_frequency, ?start_time, ?duration, "Decoding ISS SSTV");

    if fm_frequency - 0.5 * fm_bandwidth < center_frequency - 0.5 * sample_rate
        || fm_frequency + 0.5 * fm_bandwidth > center_frequency + 0.5 * sample_rate
    {
        bail!(
            "FM frequency ({}) with FM bandwidth ({}) is out of range [{}, {}]",
            si::format(fm_frequency, "Hz"),
            si::format(fm_bandwidth, "Hz"),
            si::format(center_frequency - 0.5 * sample_rate, "Hz"),
            si::format(center_frequency + 0.5 * sample_rate, "Hz")
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
        fm_frequency,
        start_time,
        predict_options.interval,
        source.sample_rate(),
        duration,
    )?;

    // apply constant frequency shift to correction to shift FM frequency to DC
    doppler_correction.fixed_offset = center_frequency - fm_frequency;

    // signal is shifted to DC - taking doppler shift into account
    let signal = source.scan_in_place_with(doppler_correction);

    // lowpass filter
    let lowpass = biquad::lowpass(sample_rate, 0.5 * fm_bandwidth);
    let signal = signal.scan_in_place_with(lowpass);

    // decimate to ~ FM bandwidth
    let signal = signal.decimate_to(fm_bandwidth);
    let sample_rate = signal.sample_rate();

    // FM demodulate
    let signal = signal.scan_with(FmDemodulator::new(sample_rate, 0.5 * fm_bandwidth));

    // write FM demodulated audio to file
    tracing::info!("Demodulating...");

    tracing::info!(
        sample_rate = %si::format(signal.sample_rate(), "Hz"),
        "output"
    );

    write_stream_to_wav(&output, signal).await?;

    Ok(())
}
