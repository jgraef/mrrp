use chrono::TimeDelta;
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    geo::Geodetic,
    track::TrackerOptions,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub station: Option<StationConfig>,

    #[serde(default)]
    pub tracker: TrackerConfig,
}

impl Config {
    pub fn tracker_options(&self) -> TrackerOptions {
        TrackerOptions {
            look_back: self.tracker.look_back,
            look_ahead: self.tracker.look_ahead,
            time_resolution: self.tracker.time_resolution,
            base_station: self.station.as_ref().map(|station| station.location),
            min_elevation: self.tracker.min_elevation,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StationConfig {
    pub location: Geodetic,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TrackerConfig {
    pub look_back: TimeDelta,
    pub look_ahead: TimeDelta,
    pub time_resolution: TimeDelta,
    pub min_elevation: f64,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            look_back: TimeDelta::days(1),
            look_ahead: TimeDelta::days(2),
            time_resolution: TimeDelta::seconds(10),
            min_elevation: 5.0f64.to_radians(),
        }
    }
}
