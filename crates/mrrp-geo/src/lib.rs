pub mod qth;

pub use qth::Qth;

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
    #[serde(default, skip_serializing_if = "f64_is_zero")]
    pub altitude: f64,
}

impl Geodetic {
    #[inline]
    pub fn as_horizontal(&self) -> HorizontalGeodetic {
        HorizontalGeodetic {
            latitude: self.latitude,
            longitude: self.longitude,
        }
    }
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
    #[inline]
    pub fn with_altitude(&self, altitude: f64) -> Geodetic {
        Geodetic {
            latitude: self.latitude,
            longitude: self.longitude,
            altitude,
        }
    }
}

impl From<Geodetic> for HorizontalGeodetic {
    #[inline]
    fn from(value: Geodetic) -> Self {
        value.as_horizontal()
    }
}

#[derive(Clone, Copy, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Orientation {
    /// Azimuth in degrees
    pub azimuth: f64,

    /// Elevation in degrees
    #[serde(default, skip_serializing_if = "f64_is_zero")]
    pub elevation: f64,

    /// Skew in degrees
    #[serde(default, skip_serializing_if = "f64_is_zero")]
    pub skew: f64,
}

fn f64_is_zero(x: &f64) -> bool {
    *x == 0.0
}
