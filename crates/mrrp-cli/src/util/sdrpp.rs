use std::path::Path;

use anyhow::{
    Error,
    anyhow,
    bail,
};
use chrono::{
    DateTime,
    Local,
    MappedLocalTime,
    NaiveDate,
    NaiveDateTime,
    NaiveTime,
    Utc,
};

use crate::util::SourceInfo;

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

    // handle folds and gaps in time
    //
    // see: [`MappedLocalTime.html`]
    let timestamp = match NaiveDateTime::new(date, time).and_local_timezone(Local) {
        MappedLocalTime::Single(timestamp) => timestamp,
        MappedLocalTime::Ambiguous(earliest, latest) => {
            let earliest_utc = DateTime::<Utc>::from(earliest);
            let latest_utc = DateTime::<Utc>::from(latest);
            tracing::warn!(
                "Parsed local time {date} {time}, which is ambigious because of a time fold. The time could mean {earliest_utc:?} or {latest_utc:?} in UTC. Defaulting to {earliest_utc:?}. You need to fix the resulting filename to account for this."
            );
            earliest
        }
        MappedLocalTime::None => {
            bail!(
                "The time stamp {date} {time} does not exist in the local timezone. Were the input files generated with a different time zone set?"
            );
        }
    };

    Ok(FileInfo {
        recording_type,
        center_frequency,
        timestamp,
    })
}

#[derive(Clone, Copy, Debug)]
pub struct FileInfo {
    pub recording_type: RecordingType,
    pub center_frequency: u32,
    pub timestamp: DateTime<Local>,
}

impl FileInfo {
    pub fn source_info(&self) -> SourceInfo {
        SourceInfo {
            frequency: self.center_frequency as f32,
            start_time: DateTime::<Utc>::from(self.timestamp),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingType {
    Audio,
    Baseband,
}
