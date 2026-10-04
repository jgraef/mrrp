pub mod writer;

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::miq::container::chunk::{
    Tag,
    Tagged,
};

/// Stream ID
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct StreamId(pub(super) u32);

/// Start of a stream
///
/// After this chunk the stream with ID `stream_id` is valid.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamStart {
    pub stream_id: StreamId,

    pub sample_format: SampleFormat,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub samples: Option<u64>,

    // todo: contains center frequency, sample rate, gain, etc. look at SM2117 for inspiration.
    // some of the values specified here can be changed in-band by later chunks.
    pub stream_info: StreamInfo,
}

impl Tagged for StreamStart {
    const TAG: Tag = Tag::from_bytes_unchecked(*b"SSTA");
}

/// Metadata for a stream
///
/// This is defined in [`StreamDefinition`]. Changes can be encoded later
/// in-band with [`MetadataChange`] chunks
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct StreamInfo<U = ()> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center_frequency: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agc: Option<Agc>,

    /// SM2117 "Data set unit"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_unit: Option<String>,

    /// SM2117 "Data set scaling factor"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_scaling_factor: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter_bandwidth: Option<f32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geo_location: Option<geo::Location>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub orientation: Option<geo::Orientation>,

    /// At least one sample might be invalid.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sample_invalid: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pll_unlocked: Option<Pll>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_defined: Option<U>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(bound(deserialize = "U: Deserialize<'de>"))]
pub struct StreamInfoChange<U = ()> {
    pub stream_id: StreamId,
    pub stream_info: StreamInfo<U>,
}

impl<U> Tagged for StreamInfoChange<U> {
    const TAG: Tag = Tag::from_bytes_unchecked(*b"SICH");
}

pub const IQ_TAG: Tag = Tag::from_bytes_unchecked(*b"SIQD");

/// End of a stream
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamEnd {
    pub stream_id: StreamId,
}

impl Tagged for StreamEnd {
    const TAG: Tag = Tag::from_bytes_unchecked(*b"SEND");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SampleFormat {
    Real(SampleComponentFormat),
    Iq(SampleComponentFormat),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum SampleComponentFormat {
    U8,
    I8,
    U16,
    I16,
    U32,
    I32,
    U64,
    I64,
    F32,
    F64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Agc {
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Pll {
    Locked,
    Unlocked,
}

pub mod geo {
    // todo:
    //
    // - we kind of have these types in mrrp-sat too. share them, or use georust
    // - some fields here should be `Option`s
    //

    use serde::{
        Deserialize,
        Serialize,
    };

    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    pub struct Location {
        /// Latitude in degrees
        pub latitude: f64,

        /// Longitude in degrees
        pub longitude: f64,

        /// Altitude in meters above WGS84 ellipsoid
        pub altitude: f64,
    }

    // todo: also either share with mrrp-sat somehow or use georust

    /// Orientation of the antenna.
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    pub struct Orientation {
        pub azimuth: f64,
        pub elevation: f64,
        pub skew: f64,
    }
}
