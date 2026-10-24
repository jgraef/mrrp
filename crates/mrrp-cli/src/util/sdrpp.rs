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
        // fixme: this will panic when we parse the file with a timestamp falling inside the fold
        // 2026-10-25 0:00 to 2:00 UTC.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingType {
    Audio,
    Baseband,
}
