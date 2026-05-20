use serde::{
    Deserialize,
    Serialize,
};

use crate::geo::Geodetic;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    pub station: StationConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StationConfig {
    pub location: Geodetic,
}

impl Default for StationConfig {
    fn default() -> Self {
        Self {
            location: Geodetic {
                latitude: 48.85821988852074,
                longitude: 2.2945244377684677,
                altitude: 51.0,
            },
        }
    }
}
