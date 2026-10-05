/// Geodetic coordinates
///
/// Geodetic coordinates with altitude relative to the WGS84 ellipsoid.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Geodetic {
    /// Latitude in degrees
    pub latitude: f64,

    /// Longitude in degrees
    pub longitude: f64,

    /// Altitude in meters (above the WGS84 ellipsoid)
    pub altitude: f64,
}

/// Horizontal geodetic coordinates
///
/// Horizontal geodetic coordinates (without altitude) relative to the WGS84
/// ellipsoid.
#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HorizontalGeodetic {
    /// Latitude in degrees
    pub latitude: f64,

    /// Longitude in degrees
    pub longitude: f64,
}

impl HorizontalGeodetic {
    pub fn with_altitude(&self, altitude: f64) -> Geodetic {
        Geodetic {
            latitude: self.latitude,
            longitude: self.longitude,
            altitude,
        }
    }
}

impl From<Geodetic> for HorizontalGeodetic {
    fn from(value: Geodetic) -> Self {
        HorizontalGeodetic {
            latitude: value.latitude,
            longitude: value.longitude,
        }
    }
}
