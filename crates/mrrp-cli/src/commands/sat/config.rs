use chrono::TimeDelta;
use mrrp_geo::Qth;
use mrrp_sat::Geodetic;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub location: Option<Location>,

    #[serde(
        default = "default_min_update_interval",
        deserialize_with = "crate::util::deserialize_human_time_delta"
    )]
    pub min_update_interval: TimeDelta,

    pub satnogs_api_token: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            location: None,
            min_update_interval: default_min_update_interval(),
            satnogs_api_token: None,
        }
    }
}

fn default_min_update_interval() -> TimeDelta {
    TimeDelta::days(14)
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
