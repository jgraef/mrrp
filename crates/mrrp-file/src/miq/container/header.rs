use serde::{
    Deserialize,
    Serialize,
};

use crate::miq::container::chunk::{
    Tag,
    Tagged,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Header<S = String> {
    // MIQ container version
    pub version: Version,

    /// Specific sub-format ID and version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_format: Option<SubFormat<S>>,

    /// Whether this file is encoded in streaming fashion.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stream: bool,
}

impl<S> Tagged for Header<S> {
    const TAG: Tag = Tag::MIQH;
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubFormat<S> {
    /// ID of the sub-format, e.g. `"iq"` for IQ streams.
    pub id: S,

    /// Version of the sub-format.
    pub version: Version,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}
