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
    commands::miq::sdrpp::RecordingType,
};

pub async fn run(context: Context<Args>) -> Result<(), Error> {
    match context.args.command {
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
                if file_name_info.recording_type != RecordingType::Baseband {
                    bail!("Input file must be baseband");
                }
                stream_info.merge(file_name_info.stream_info());
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

/// QTH commands
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
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

mod sdrpp {
    use std::path::Path;

    use anyhow::{
        Error,
        anyhow,
    };
    use chrono::{
        DateTime,
        Local,
        NaiveDate,
        NaiveDateTime,
        NaiveTime,
    };
    use mrrp_file::miq::stream::StreamInfo;

    pub fn parse_path(path: impl AsRef<Path>) -> Result<FileInfo, Error> {
        let path = path.as_ref();
        let file_name = path
            .file_name()
            .ok_or_else(|| anyhow!("Could not get filename from path: {path:?}"))?
            .to_str()
            .ok_or_else(|| anyhow!("Filename contains invalid UTF-8: {path:?}"))?;
        parse_file_name(file_name)
    }

    pub fn parse_file_name(file_name: &str) -> Result<FileInfo, Error> {
        macro_rules! invalid {
            () => {
                anyhow!("Could not parse file name: {file_name}")
            };
        }

        let file_name = file_name.trim_end_matches(".wav");
        let mut parts = file_name.split('_');

        let recording_type = match parts.next() {
            Some("audio") => RecordingType::Audio,
            Some("baseband") => RecordingType::Baseband,
            _ => return Err(invalid!()),
        };

        let center_frequency = parts.next().ok_or_else(|| invalid!())?;
        let center_frequency = center_frequency.trim_end_matches("Hz");
        let center_frequency: u32 = center_frequency.parse()?;

        let time = NaiveTime::parse_from_str(parts.next().ok_or_else(|| invalid!())?, "%H-%M-%S")?;
        let date = NaiveDate::parse_from_str(parts.next().ok_or_else(|| invalid!())?, "%d-%m-%Y")?;
        let timestamp = NaiveDateTime::new(date, time)
            .and_local_timezone(Local)
            .unwrap();

        Ok(FileInfo {
            recording_type,
            center_frequency,
            timestamp,
        })
    }

    #[derive(Debug)]
    pub struct FileInfo {
        pub recording_type: RecordingType,
        pub center_frequency: u32,
        pub timestamp: DateTime<Local>,
    }

    impl FileInfo {
        pub fn stream_info<U>(&self) -> StreamInfo<U> {
            StreamInfo {
                center_frequency: Some(self.center_frequency as f32),
                timestamp: Some(self.timestamp.to_utc()),
                ..Default::default()
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum RecordingType {
        Audio,
        Baseband,
    }
}
