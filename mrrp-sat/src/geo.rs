use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Geodetic {
    /// Latitude in degrees
    pub latitude: f64,

    /// Longitude in degrees
    pub longitude: f64,

    /// Altitude in meters above WGS84 ellipsoid
    pub altitude: f64,
}
