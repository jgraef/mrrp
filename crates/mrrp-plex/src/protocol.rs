use mrrp_core::signal::Frequency;
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceListEntry {
    // todo
}

#[derive(
    Clone,
    Copy,
    Debug,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    derive_more::Display,
    derive_more::FromStr,
)]
#[serde(transparent)]
pub struct StreamId(Uuid);

impl StreamId {
    #[inline]
    pub(crate) fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamStart {
    pub stream_id: StreamId,
    pub stream_info: StreamInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct StreamInfo {
    pub center_frequency: Option<Frequency>,
    pub sample_rate: Frequency,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StreamEvent {
    InfoChanged { stream_info: StreamInfo },
    CenterFrequencyChanged { center_frequency: Frequency },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum StreamCommand {
    SetCenterFrequency { center_frequency: Frequency },
    SetSampleRate { sample_rate: Frequency },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamListEntry {
    pub stream_id: StreamId,
    pub stream_info: StreamInfo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Event {
    StreamList {
        streams: Vec<StreamListEntry>,
    },
    StreamCreated {
        stream_id: StreamId,
        stream_info: StreamInfo,
    },
    StreamClosed {
        stream_id: StreamId,
    },
    StreamEvent {
        stream_id: StreamId,
        event: StreamEvent,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateStreamOptions {
    // todo
}
