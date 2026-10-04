use std::{
    borrow::Cow,
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::{
    Error,
    anyhow,
    bail,
};
use clap::{
    Parser,
    Subcommand,
    ValueEnum,
};
use futures_util::pin_mut;
use mrrp_core::{
    buf::SampleBufMut,
    sample::{
        Complex,
        encoding::BigEndian,
    },
    signal::GetSampleRate,
};
use mrrp_file::{
    miq::{
        codec::pcm::Pcm,
        container::reader::Reader as MiqContainerReader,
        stream::{
            StreamInfo,
            writer::Writer as MiqWriter,
        },
    },
    wav::WavSource,
};
use mrrp_util::signal::AsyncReadSamplesExt;

use crate::{
    Context,
    util::{
        SourceInfo,
        sdrpp,
    },
};

/// MIQ file and stream tools
#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Clone, Debug, Subcommand)]
enum Command {
    Encode {
        /// Output file
        #[clap(short, long)]
        output: Option<PathBuf>,

        /// Input file (signed 16bit WAV)
        input: PathBuf,

        /// Attach stream info from TOML file.
        #[clap(short = 'i', long)]
        stream_info: Option<PathBuf>,

        /// Extract information from file name.
        #[clap(short = 'I', long)]
        file_name_info: Option<FileNameInfoKind>,

        /// Overwrite output file if it already exists.
        #[clap(short, long)]
        force: bool,
    },
    Inspect {
        file: PathBuf,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum FileNameInfoKind {
    #[clap(alias = "sdr++")]
    Sdrpp,
}

pub async fn run(args: Args, _context: Context) -> Result<(), Error> {
    match args.command {
        Command::Encode {
            output,
            input,
            stream_info,
            file_name_info,
            force,
        } => {
            encode(output, input, stream_info, file_name_info, force).await?;
        }
        Command::Inspect { file } => {
            inspect(file)?;
        }
    }

    Ok(())
}

async fn encode(
    output: Option<impl AsRef<Path>>,
    input: impl AsRef<Path>,
    stream_info_file: Option<impl AsRef<Path>>,
    file_name_info_kind: Option<FileNameInfoKind>,
    force: bool,
) -> Result<(), Error> {
    let mut stream_info: StreamInfo<toml::Value> = Default::default();

    // extract stream info from file name
    if let Some(file_name_info_kind) = file_name_info_kind {
        match file_name_info_kind {
            FileNameInfoKind::Sdrpp => {
                let file_name_info = sdrpp::parse_path(&input)?;
                tracing::debug!(?file_name_info, "extracted info from file name");
                if file_name_info.recording_type != sdrpp::RecordingType::Baseband {
                    bail!("Input file must be baseband");
                }
                let mut stream_info = Default::default();
                merge_source_info(&mut stream_info, &file_name_info.source_info());
            }
        }
    }

    // read stream info from file
    if let Some(stream_info_file) = stream_info_file {
        let file_stream_info: StreamInfo<toml::Value> =
            toml::from_slice(&std::fs::read(&stream_info_file)?)?;
        stream_info.merge(file_stream_info);
    }

    let output = if let Some(output) = output.as_ref() {
        Cow::Borrowed(output.as_ref())
    }
    else {
        let directory = input
            .as_ref()
            .parent()
            .ok_or_else(|| anyhow!("Input file has no parent directory"))?;
        let file_prefix = input
            .as_ref()
            .file_prefix()
            .ok_or_else(|| anyhow!("Can't determine input file prefix"))?
            .to_string_lossy();
        let output = directory.join(format!("{file_prefix}.miq"));

        if output.exists() && !force {
            bail!("The derive output file already exists: {output:?}");
        }

        Cow::Owned(output)
    };

    let input = WavSource::<_, Complex<i16>>::from_path(&input)?;
    pin_mut!(input);

    if stream_info.sample_rate.is_none() {
        stream_info.sample_rate = Some(input.sample_rate());
    }

    let mut output = MiqWriter::from_path(&output)?;
    let mut stream_writer =
        output.start_stream::<Complex<i16>, _, _>(stream_info, Pcm::<BigEndian>::default())?;

    let chunk_size = 0x10000; // 64 kB
    let mut buffer = Vec::with_capacity(chunk_size);
    loop {
        buffer.clear();

        input
            .read_into_buf(&mut (&mut buffer).limit(chunk_size))
            .await?;
        if buffer.is_empty() {
            break;
        }

        assert!(buffer.len() <= chunk_size, "{}", buffer.len());

        stream_writer.write_samples(&mut output, &buffer)?;
    }

    Ok(())
}

fn inspect(file: impl AsRef<Path>) -> Result<(), Error> {
    let mut reader = MiqContainerReader::from_path(&file)?;

    let file_header = reader.read_file_header()?;
    println!("{file_header:?}");

    while let Some(chunk) = reader.try_read_chunk()? {
        let chunk_header = chunk.chunk_header();
        println!("{chunk_header:?}");
    }

    Ok(())
}

pub fn merge_source_info(stream_info: &mut StreamInfo, source_info: &SourceInfo) {
    stream_info.center_frequency = Some(source_info.frequency);
    stream_info.timestamp = Some(source_info.start_time);
}
