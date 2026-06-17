use std::path::Path;

use anyhow::Error;
use indexmap::IndexMap;
use serde::Deserialize;

const DEFAULT_CONFIG_TOML: &str = include_str!("../config.example.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub radios: IndexMap<String, RadioConfig>,
}

impl Config {
    pub fn read_or_default(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        let config = if path.exists() {
            let toml = std::fs::read(&path)?;
            toml::from_slice(&toml)?
        }
        else {
            std::fs::write(path, DEFAULT_CONFIG_TOML)?;
            toml::from_str(DEFAULT_CONFIG_TOML).expect("invalid default config")
        };

        Ok(config)
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RadioConfig {
    RtlSdr {
        #[serde(flatten)]
        filter: RtlSdrDeviceFilter,

        sample_rate: Option<u64>,

        #[serde(default)]
        bias_tee: bool,
    },
    RtlTcp {
        hostname: String,
        port: u16,

        sample_rate: Option<u64>,
    },
    Audio {
        // todo: some soundcard input and optional rigctl client
    },
    Network {
        // todo: some network input and optional rigctl client
    },
}

#[derive(Clone, Debug, Deserialize)]
pub struct RtlSdrDeviceFilter {
    pub index: Option<usize>,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
}
