use std::time::Duration;

use anyhow::Error;
use chrono::{
    DateTime,
    Utc,
};
use mrrp_sat::{
    Geodetic,
    pass::{
        CompletePass,
        PassEvent,
        PassEvents,
        PassEventsOptions,
    },
    satellite::Satellite,
};
use tabled::{
    Table,
    Tabled,
};

use crate::TableStyle;

#[derive(Clone, Debug, clap::Args)]
pub struct Options {
    #[clap(short, long)]
    pub start_time: Option<DateTime<Utc>>,

    #[clap(short, long)]
    pub end_time: Option<DateTime<Utc>>,

    #[clap(short = 'n', long)]
    pub limit: Option<usize>,

    #[clap(short, long, default_value = "5s", value_parser = humantime::parse_duration)]
    pub predict_interval: Duration,

    #[clap(short = 'b', long, default_value = "1024")]
    pub batch_size: usize,

    #[clap(
        short = 'e',
        long,
        default_value = "0",
        value_parser = crate::util::parse_degrees_as_radians::<f64>,
    )]
    pub min_elevation: f64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            start_time: None,
            end_time: None,
            limit: Some(20),
            predict_interval: Duration::from_secs(30),
            batch_size: 1024,
            min_elevation: 10.0f64.to_radians(),
        }
    }
}

pub fn list(
    satellite: &Satellite,
    location: Geodetic,
    options: Options,
    table_style: TableStyle,
) -> Result<(), Error> {
    let mut rows = vec![];

    let passes = PassEvents::new(
        satellite,
        location,
        PassEventsOptions {
            start_time: options.start_time,
            predict_interval: options.predict_interval,
            batch_size: options.batch_size,
            min_elevation: options.min_elevation,
        },
    );

    for result in passes {
        match result? {
            PassEvent::End(complete) => {
                if options
                    .end_time
                    .is_some_and(|end_time| complete.start.state.time() > end_time)
                    || options.limit.is_some_and(|limit| limit >= rows.len())
                {
                    break;
                }

                rows.push(TableRow::from(complete));
            }
            _ => {}
        }
    }

    let mut table = Table::new(&rows);
    table.with(table_style);
    println!("{}", table);

    Ok(())
}

#[derive(Debug, Tabled)]
struct TableRow {
    #[tabled(rename = "AOS")]
    aos: DateTime<Utc>,

    #[tabled(rename = "TCA")]
    tca: DateTime<Utc>,

    #[tabled(rename = "LOS")]
    los: DateTime<Utc>,

    #[tabled(rename = "Duration")]
    duration: humantime::Duration,

    #[tabled(rename = "Max El")]
    max_elevation: f64,

    #[tabled(rename = "AOS Az")]
    aos_azimuth: f64,

    #[tabled(rename = "Max El Az")]
    max_elevation_azimuth: f64,

    #[tabled(rename = "LOS Az")]
    los_azimuth: f64,
}

impl From<CompletePass> for TableRow {
    fn from(pass: CompletePass) -> Self {
        Self {
            aos: pass.start.state.time(),
            tca: pass.closest_approach.state.time(),
            los: pass.end.state.time(),
            duration: (pass.end.state.time() - pass.start.state.time())
                .to_std()
                .unwrap()
                .into(),
            max_elevation: pass.max_elevation.relative.elevation().to_degrees(),
            aos_azimuth: pass.start.relative.azimuth().to_degrees(),
            max_elevation_azimuth: pass.max_elevation.relative.azimuth().to_degrees(),
            los_azimuth: pass.end.relative.azimuth().to_degrees(),
        }
    }
}
