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
        PassDetector,
        PassEvent,
    },
    satellite::{
        OrbitPropagationCache,
        ReferenceState,
        Satellite,
    },
};
use tabled::{
    Table,
    Tabled,
};

#[derive(Clone, Debug, clap::Args)]
pub struct Options {
    #[clap(short, long)]
    pub start_time: Option<DateTime<Utc>>,

    #[clap(short, long)]
    pub end_time: Option<DateTime<Utc>>,

    #[clap(short = 'n', long)]
    pub limit: Option<usize>,

    #[clap(short, long, default_value = "30s", value_parser = humantime::parse_duration)]
    pub predict_interval: Duration,

    #[clap(short = 'b', long, default_value = "1024")]
    pub batch_size: usize,

    #[clap(
        short = 'e',
        long,
        default_value = "10",
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

pub fn list(satellite: &Satellite, location: Geodetic, options: Options) -> Result<(), Error> {
    let mut time = options.start_time.unwrap_or_else(Utc::now);
    let mut times = vec![];

    let mut orbit_propagation_cache = OrbitPropagationCache::default();
    let mut pass_detector = PassDetector::new(
        options.min_elevation,
        ReferenceState::from_geodetic(location),
    );

    let mut rows = vec![];

    while options.limit.is_none_or(|limit| rows.len() < limit) {
        // fill times buffer with next batch of timestamps
        fill_times(
            &mut time,
            options.end_time,
            options.predict_interval,
            options.batch_size,
            &mut times,
        );

        // if fill_times returns with an empty buffer, we are past
        // `options.end_time`
        if times.is_empty() {
            break;
        }

        // predict satellite states
        let satellite_states = satellite.predict_state(&times, &mut orbit_propagation_cache)?;

        for state in &satellite_states {
            match pass_detector.push(*state) {
                Some(PassEvent::End(complete)) => {
                    rows.push(TableRow::from(complete));
                }
                _ => {}
            }
        }
    }

    println!("{}", Table::new(&rows));

    Ok(())
}

fn fill_times(
    time: &mut DateTime<Utc>,
    end_time: Option<DateTime<Utc>>,
    predict_interval: Duration,
    batch_size: usize,
    buffer: &mut Vec<DateTime<Utc>>,
) {
    buffer.clear();
    buffer.reserve(batch_size);

    for _ in 0..batch_size {
        if end_time.is_some_and(|end_time| *time > end_time) {
            break;
        }
        buffer.push(*time);
        *time += Duration::from_secs_f32(predict_interval.as_secs_f32());
    }
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
