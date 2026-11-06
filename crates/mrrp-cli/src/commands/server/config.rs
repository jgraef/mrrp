use regex::Regex;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_listen_address")]
    pub listen_address: String,

    #[serde(default)]
    pub receivers: Vec<ReceiverConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
            receivers: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReceiverConfig {
    pub alias: Option<String>,

    #[serde(flatten)]
    pub device_specific: DeviceSpecificReceiverConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum DeviceSpecificReceiverConfig {
    RtlSdr(RtlSdrReceiverConfig),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RtlSdrReceiverConfig {
    pub device_name: Option<StringMatch>,
    pub serial: Option<StringMatch>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
}

impl RtlSdrReceiverConfig {
    pub fn matches(&self, device_info: &mrrp_rtl_sdr::enumerate::DeviceInfo) -> bool {
        self.device_name
            .as_ref()
            .is_none_or(|device_name| device_name.matches(device_info.name()))
            && self.serial.as_ref().is_none_or(|serial| {
                device_info
                    .serial_number()
                    .is_some_and(|device_serial| serial.matches(device_serial))
            })
            && self
                .vendor_id
                .is_none_or(|vendor_id| vendor_id == device_info.vendor_id())
            && self
                .product_id
                .is_none_or(|product_id| product_id == device_info.product_id())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringMatch {
    Regex {
        #[serde(with = "crate::util::serde::regex")]
        regex: Regex,
    },
    Verbatim(String),
}

impl StringMatch {
    fn matches(&self, other: impl AsRef<str>) -> bool {
        match self {
            StringMatch::Regex { regex } => regex.is_match(other.as_ref()),
            StringMatch::Verbatim(expected) => expected == other.as_ref(),
        }
    }
}

fn default_listen_address() -> String {
    "localhost:7474".to_owned()
}
