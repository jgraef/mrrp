use std::path::{
    Path,
    PathBuf,
};

use anyhow::{
    Error,
    anyhow,
};
use directories::ProjectDirs;

#[derive(Clone, Debug)]
pub struct Directories {
    directories: ProjectDirs,
    state_dir: PathBuf,
}

impl Directories {
    pub fn new() -> Result<Self, Error> {
        let directories = ProjectDirs::from("", "mrrp", "mrrp-sdr")
            .ok_or_else(|| anyhow!("Can't determine project directories"))?;

        let state_dir = directories.state_dir().map_or_else(
            || directories.data_local_dir().join("state"),
            |state_dir| state_dir.to_owned(),
        );
        std::fs::create_dir_all(&state_dir)?;
        std::fs::create_dir_all(directories.config_dir())?;

        Ok(Self {
            directories,
            state_dir,
        })
    }

    pub fn config_path(&self) -> PathBuf {
        self.directories.config_dir().join("config.toml")
    }

    pub fn state_dir(&self) -> &Path {
        &self.state_dir
    }
}
