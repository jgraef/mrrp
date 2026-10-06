use std::path::PathBuf;

use anyhow::{
    Error,
    anyhow,
    bail,
};
use clap::{
    Parser,
    Subcommand,
};
use futures_util::pin_mut;
use mrrp_core::{
    buf::SamplesMut,
    sample::{
        Complex,
        encoding::BigEndian,
    },
};
use mrrp_file::{
    miq::{
        codec::pcm::Pcm,
        stream::{
            SampleComponentFormat,
            SampleFormat,
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
        Command::Encode { output, input } => {
            let info = sdrpp::parse_path(&input)?;
            tracing::debug!(?info, "extracted info from file name");

            if info.recording_type != RecordingType::Baseband {
                bail!("Input file must be baseband");
            }

            let mut stream_info = info.stream_info();
            //stream_info.sample_rate =

            let input = WavSource::<_, Complex<i16>>::from_path(&input)?;
            pin_mut!(input);

            let mut output = MiqWriter::from_path(&output)?;
            let mut stream_writer =
                output.start_stream::<Complex<i16>, _>(stream_info, Pcm::<BigEndian>::default())?;

            // todo: test if this works with a Vec with 0 initial capacity
            let mut buffer = Vec::with_capacity(0x1000);
            loop {
                buffer.clear();
                input.read_into_buf(&mut buffer).await?;
                let mut iq_writer = stream_writer.start_write(&mut output)?;
            }

            // I really want a Vec<Sample> with limited size :/
            assert_eq!(buffer.capacity(), 0x1000);
        }
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
        #[clap(short, long)]
        output: PathBuf,

        input: PathBuf,
    },
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
        pub fn stream_info(&self) -> StreamInfo {
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
