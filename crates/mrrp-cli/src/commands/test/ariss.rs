use std::{
    fs::File,
    io::BufWriter,
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
use mrrp_filter::{
    AsyncReadSamplesFilterExt,
    biquad,
};
use mrrp_geo::Geodetic;
use mrrp_modem::fm::FmDemodulator;
use mrrp_sat::{
    doppler_correct::DopplerCorrection,
    satellite::{
        ReferenceState,
        Satellite,
    },
    satnogs::SatelliteId,
};
use mrrp_sstv::modes::VisCode;
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    FuncScanner,
    Scanner,
    ScannerExt,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    commands::{
        Context,
        modem::sstv::vis::VisDetect,
        sat::{
            PredictOptions,
            get_satellite,
        },
    },
    util::{
        SourceInfo,
        SourceInfoOptions,
        run_til_shutdown,
        si,
    },
};

/// Experimental tools
#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, Subcommand)]
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

        /// Path to info file
        #[clap(short, long)]
        info: Option<PathBuf>,

        /// Path for demodulated output
        #[clap(short, long)]
        output: PathBuf,

        /// Input file path
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

        /// Audio output directory
        ///
        /// By default the audio output files are written to the input
        /// directory.
        #[clap(short, long)]
        audio: Option<PathBuf>,

        /// Info output directory.
        ///
        /// When specified JSON files containing metadata are written to this
        /// directoryr.
        #[clap(short, long)]
        info: Option<PathBuf>,

        /// Input directory
        baseband: PathBuf,
    },
}

#[derive(Clone, Debug, clap::Args)]
struct DemodOptions {
    #[clap(short, long, value_parser = si::parse_frequency::<f32>)]
    fm_frequency: f32,

    #[clap(short = 'b', long, value_parser = si::parse_frequency::<f32>)]
    fm_bandwidth: f32,

    #[clap(flatten)]
    predict_options: PredictOptions,
}

pub async fn run(args: Args, context: Context) -> Result<(), Error> {
    match args.command {
        Command::Demod {
            demod_options,
            source_info_options,
            sat_id,
            info,
            input,
            output,
        } => {
            let source_info = SourceInfo::from_path_or_options(&input, &source_info_options)?;

            let satellites = context.open_satellite_db().await?;
            let location = context.get_geolocation()?;
            let satellite = get_satellite(&satellites, &sat_id)?;

            let result = demod(
                &input,
                &output,
                satellite,
                location,
                &source_info,
                &demod_options,
            )
            .await?;

            if let Some(sstv_detections) = result
                && let Some(info) = &info
            {
                let input_filename = to_filename(&input)?;
                let output_filename = to_filename(&output)?;
                write_demod_info(
                    info,
                    &input_filename,
                    &output_filename,
                    &source_info,
                    &demod_options,
                    sstv_detections,
                )?;
            }
        }
        Command::DemodAll {
            demod_options,
            sat_id,
            audio: audio_dir,
            info: info_dir,
            baseband: baseband_dir,
        } => {
            if !baseband_dir.is_dir() {
                bail!("Baseband input path is not a directory: {baseband_dir:?}");
            }
            let audio_dir = audio_dir.as_ref().map_or_else(
                || Ok(&baseband_dir),
                |audio_dir| {
                    if !audio_dir.is_dir() {
                        bail!("Audio output path is not a directory: {audio_dir:?}");
                    }
                    Ok(audio_dir)
                },
            )?;
            if let Some(info_dir) = &info_dir
                && !info_dir.is_dir()
            {
                bail!("Info output path is not a directory: {info_dir:?}");
            }

            let satellites = context.open_satellite_db().await?;
            let location = context.get_geolocation()?;
            let satellite = get_satellite(&satellites, &sat_id)?;

            for result in std::fs::read_dir(&baseband_dir)? {
                let dir_entry = result?;
                let baseband_path = dir_entry.path();
                let baseband_filename = to_filename(&baseband_path)?;
                let baseband_metadata = dir_entry.metadata()?;

                if baseband_filename.starts_with("baseband") {
                    let source_info = SourceInfo::from_path(&baseband_path)?;
                    tracing::debug!(filename = ?baseband_filename, ?source_info, "processing baseband");

                    let audio_info = source_info.with_frequency(demod_options.fm_frequency);
                    let audio_filename = audio_info.filename("audio", "wav");
                    let audio_path = audio_dir.join(&audio_filename);

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

                    let result = demod(
                        &baseband_path,
                        &audio_path,
                        satellite,
                        location,
                        &source_info,
                        &demod_options,
                    )
                    .await?;

                    if let Some(sstv_detections) = result {
                        if let Some(info_dir) = &info_dir {
                            let info_filename = audio_info.filename("info", ".json");
                            let info_path = info_dir.join(&info_filename);

                            write_demod_info(
                                &info_path,
                                baseband_filename,
                                &audio_filename,
                                &source_info,
                                &demod_options,
                                sstv_detections,
                            )?;
                        }
                    }
                    else {
                        // cancelled
                        break;
                    }
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
) -> Result<Option<Vec<SstvDetection>>, Error> {
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
    let source = source.progress(FuncScanner::new(|num_samples: usize| {
        progress_bar.inc(num_samples as u64);
    }));

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
    let lowpass = biquad::lowpass(sample_rate, 0.5 * demod_options.fm_bandwidth)?;
    let signal = signal.scan_in_place_with(lowpass);

    // decimate to ~ FM bandwidth
    let signal = signal.decimate_to(demod_options.fm_bandwidth);
    let sample_rate = signal.sample_rate();

    // setup VIS header buffer
    let mut vis_collector = VisCollector::new(sample_rate);

    // FM demodulate
    let signal = signal.scan_with(FmDemodulator::new(
        sample_rate,
        0.5 * demod_options.fm_bandwidth,
    ));
    //let lowpass = biquad::lowpass(sample_rate, 0.25 *
    // demod_options.fm_bandwidth)?;
    let highpass = biquad::highpass(sample_rate, 10.0)?;
    let clamp = Clamp::symmetric(0.5);
    let signal = signal.scan_in_place_with(clamp.chain(highpass).chain(&mut vis_collector));

    // write FM demodulated audio to file
    let signal = signal.convert::<i16>();
    tracing::info!(?output, sample_rate = %si::format(sample_rate, "Hz"), "Writing demodulated audio to file");

    // we use `run_until_shutdown` to gracefully abort on Ctrl-C.
    //
    // on abort we could just finish writing the file. that would mean we need
    // to make the signal we pass to `write_stream_to_wav` cancelable. a
    // `write_stream_to_wav_with_graceful_shutdown` would be nice to have.
    if run_til_shutdown(async move { write_stream_to_wav(&output, signal).await })
        .await
        .transpose()?
        .is_none()
    {
        tracing::info!(?output, "Processing was cancelled. Deleting file.");
        std::fs::remove_file(&output)?;
        return Ok(None);
    }

    Ok(Some(vis_collector.finish()))
}

#[derive(Clone, Copy, Debug)]
pub struct Clamp {
    pub min: f32,
    pub max: f32,
}

impl Clamp {
    pub fn new(min: f32, max: f32) -> Self {
        Self { min, max }
    }

    pub fn symmetric(limit: f32) -> Self {
        Self {
            min: -limit,
            max: limit,
        }
    }
}

impl Scanner<f32> for Clamp {
    type Output = f32;

    fn scan(&mut self, sample: f32) -> Self::Output {
        sample.clamp(self.min, self.max)
    }
}

#[derive(Clone, Debug)]
pub struct VisCollector {
    vis_detect: VisDetect,
    vis_headers: Vec<SstvDetection>,
    sample_rate: f32,
    sample_num: usize,
}

impl VisCollector {
    pub fn new(sample_rate: f32) -> Self {
        Self {
            vis_detect: VisDetect::new(sample_rate),
            vis_headers: vec![],
            sample_rate,
            sample_num: 0,
        }
    }

    pub fn finish(self) -> Vec<SstvDetection> {
        self.vis_headers
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SstvDetection {
    pub vis_code: VisCode,
    pub sample_num: usize,
    pub time: Duration,
}

impl Scanner<f32> for VisCollector {
    type Output = f32;

    fn scan(&mut self, sample: f32) -> Self::Output {
        if let Some(vis_code) = self.vis_detect.scan(sample) {
            let time = Duration::from_secs_f32(self.sample_num as f32 / self.sample_rate);

            if let Some(vis_code) = vis_code.check_parity() {
                tracing::info!(?vis_code, sample_num = ?self.sample_num, ?time, "detected VIS header");
                self.vis_headers.push(SstvDetection {
                    vis_code,
                    sample_num: self.sample_num,
                    time,
                });
            }
            else {
                tracing::info!(?vis_code, sample_num = ?self.sample_num, ?time, "detected VIS header, but invalid parity");
            }
        }
        self.sample_num += 1;

        sample
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DemodInfo {
    pub baseband: String,
    pub audio: String,
    pub start_time: DateTime<Utc>,
    pub center_frequency: f32,
    pub fm_frequency: f32,
    pub fm_bandwidth: f32,
    pub sstv_detections: Vec<SstvDetection>,
}

fn write_demod_info(
    path: impl AsRef<Path>,
    baseband_filename: &str,
    audio_filename: &str,
    source_info: &SourceInfo,
    demod_options: &DemodOptions,
    sstv_detections: Vec<SstvDetection>,
) -> Result<(), Error> {
    let demod_info = DemodInfo {
        baseband: baseband_filename.to_owned(),
        audio: audio_filename.to_owned(),
        start_time: source_info.start_time,
        center_frequency: source_info.frequency,
        fm_frequency: demod_options.fm_frequency,
        fm_bandwidth: demod_options.fm_bandwidth,
        sstv_detections,
    };

    tracing::debug!(path = ?path.as_ref(), "writing info file");
    let writer = BufWriter::new(File::create(&path)?);
    serde_json::to_writer_pretty(writer, &demod_info)?;

    Ok(())
}

fn to_filename<'a>(path: &'a Path) -> Result<&'a str, Error> {
    let filename = path
        .file_name()
        .ok_or_else(|| anyhow!("Path has no filename: {path:?}"))?;
    filename
        .to_str()
        .ok_or_else(|| anyhow!("Invalid filename: {filename:?}"))
}
