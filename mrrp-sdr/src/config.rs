use std::path::Path;

use anyhow::Error;
use serde::Deserialize;

const DEFAULT_CONFIG_TOML: &str = include_str!("../config.default.toml");

#[derive(Clone, Debug, Deserialize)]
pub struct Config {
    // todo
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
