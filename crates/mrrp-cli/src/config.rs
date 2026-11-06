use serde::{
    Deserialize,
    Serialize,
};

use crate::commands;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub sat: commands::sat::Config,

    #[serde(default)]
    pub plex: commands::server::Config,
}
