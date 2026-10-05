use chrono::TimeDelta;
use mrrp_geo::Qth;
use mrrp_sat::{
    Geodetic,
    tracker::TrackerOptions,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub location: Option<Location>,

    #[serde(default)]
    pub tracker: TrackerConfig,

    #[serde(
        default = "default_min_update_interval",
        deserialize_with = "crate::util::deserialize_human_time_delta"
    )]
    pub min_update_interval: TimeDelta,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            location: None,
            tracker: Default::default(),
            min_update_interval: default_min_update_interval(),
        }
    }
}

fn default_min_update_interval() -> TimeDelta {
    TimeDelta::days(14)
}

impl Config {
    pub fn tracker_options(&self) -> TrackerOptions {
        TrackerOptions {
            look_back: self.tracker.look_back,
            look_ahead: self.tracker.look_ahead,
            time_resolution: self.tracker.time_resolution,
            base_station: self.location.map(|location| location.to_geodetic()),
            min_elevation: self.tracker.min_elevation,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Location {
    Geodetic(Geodetic),
    Qth {
        qth: Qth,
        #[serde(default)]
        altitude: f64,
    },
}

impl Location {
    pub fn to_geodetic(&self) -> Geodetic {
        match self {
            Location::Geodetic(geodetic) => *geodetic,
            Location::Qth { qth, altitude } => qth.geodetic().with_altitude(*altitude),
        }
    }
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
