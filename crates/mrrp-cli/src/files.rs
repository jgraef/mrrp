use std::path::Path;

use anyhow::{
    Error,
    anyhow,
};
use directories::ProjectDirs;

use crate::config::Config;

#[derive(Debug)]
pub struct Files {
    dirs: ProjectDirs,
}

impl Files {
    pub fn open() -> Result<Self, Error> {
        let dirs = ProjectDirs::from("", "mrrp", "mrrp-cli")
            .ok_or_else(|| anyhow!("Failed to determine project directories"))?;

        create_dir_if_not_exists(dirs.config_dir())?;
        create_dir_if_not_exists(dirs.data_dir())?;

        Ok(Self { dirs })
    }

    pub fn config(&self) -> Result<Config, Error> {
        let config_path = self.dirs.config_dir().join("config.toml");

        let config = if !config_path.exists() {
            tracing::info!(?config_path, "Config not found. Creating default config.");
            let config = Config::default();
            std::fs::write(&config_path, &toml::to_string_pretty(&config)?)?;
            config
        }
        else {
            tracing::info!(?config_path, "Reading config");
            toml::from_slice(&std::fs::read(&config_path)?)?
        };

        Ok(config)
    }

    pub fn data_dir(&self) -> &Path {
        self.dirs.data_dir()
    }
}

pub fn create_dir_if_not_exists(path: impl AsRef<Path>) -> Result<(), Error> {
    let path = path.as_ref();

    if !path.exists() {
        std::fs::create_dir_all(&path)?;
    }

    Ok(())
}

pub fn create_parent_dir_if_not_exists(path: impl AsRef<Path>) -> Result<(), Error> {
    if let Some(path) = path.as_ref().parent()
        && !path.exists()
    {
        std::fs::create_dir_all(&path)?;
    }

    Ok(())
}
