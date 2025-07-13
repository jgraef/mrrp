use std::{
    fs::File,
    io::{
        BufReader,
        BufWriter,
    },
    path::{
        Path,
        PathBuf,
    },
};

use chrono::Local;
use color_eyre::eyre::eyre;
use directories::ProjectDirs;

use crate::{
    Error,
    ui::{
        UiState,
        bandplan::{
            BANDPLAN_INTERNATIONAL_BYTES,
            Bandplan,
        },
        keybinds::Keybinds,
    },
    util::Snapshot,
};

#[derive(Debug)]
pub struct AppFiles {
    project_dirs: ProjectDirs,
}

impl AppFiles {
    pub fn new() -> Result<Self, Error> {
        let project_dirs = ProjectDirs::from("", "mrrp", "mrrp-cli")
            .ok_or_else(|| eyre!("Could not determine project directories"))?;
        let this = Self { project_dirs };

        std::fs::create_dir_all(this.config_dir())?;
        std::fs::create_dir_all(this.state_dir())?;

        Ok(this)
    }

    fn config_dir(&self) -> &Path {
        self.project_dirs.config_dir()
    }

    fn state_dir(&self) -> &Path {
        self.project_dirs
            .state_dir()
            .unwrap_or_else(|| self.project_dirs.data_dir())
    }

    pub fn bandplan(&self) -> Result<Bandplan, Error> {
        let path = self.config_dir().join("bandplan.csv");

        if path.exists() {
            Bandplan::from_path(path)
        }
        else {
            tracing::debug!(path = %path.display(), "Writing default (international) bandplan to file");
            let data = BANDPLAN_INTERNATIONAL_BYTES;
            let bandplan = Bandplan::from_reader(data)?;
            std::fs::write(&path, data)?;
            Ok(bandplan)
        }
    }

    pub fn keybinds(&self) -> Result<Keybinds, Error> {
        let path = self.config_dir().join("keybinds.toml");

        if path.exists() {
            Keybinds::from_path(path)
        }
        else {
            Ok(Keybinds::default())
        }
    }

    fn ui_state_path(&self) -> PathBuf {
        self.state_dir().join("ui_state.cbor")
    }

    pub fn load_ui_state(&self) -> Result<Snapshot<UiState>, Error> {
        let path = self.ui_state_path();
        tracing::debug!(path = %path.display(), "Loading UI state");
        Ok(serde_cbor::from_reader(BufReader::new(File::open(path)?))?)
    }

    pub fn save_ui_state(&self, ui_state: &UiState) -> Result<(), Error> {
        let path = self.ui_state_path();
        tracing::debug!(path = %path.display(), "Saving UI state");
        serde_cbor::to_writer(
            BufWriter::new(File::create(path)?),
            &Snapshot {
                state: ui_state,
                timestamp: Local::now(),
            },
        )?;
        Ok(())
    }
}
