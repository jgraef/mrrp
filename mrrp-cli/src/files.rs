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

use color_eyre::eyre::eyre;
use directories::ProjectDirs;

use crate::{
    Error,
    app::{
        AppSnapshot,
        AppState,
    },
    ui::{
        bandplan::{
            BANDPLAN_INTERNATIONAL_BYTES,
            Bandplan,
        },
        bookmarks::Bookmarks,
        keybinds::Keybinds,
        waterfall::ColorMap,
    },
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
            let keybinds = Keybinds::default();
            keybinds.to_path(path)?;
            Ok(keybinds)
        }
    }

    pub fn color_map(&self) -> Result<ColorMap, Error> {
        let path = self.config_dir().join("colormap.toml");

        if path.exists() {
            ColorMap::from_path(path)
        }
        else {
            let color_map = ColorMap::default();
            color_map.to_path(path)?;
            Ok(color_map)
        }
    }

    pub fn bookmarks(&self) -> Result<Bookmarks, Error> {
        let path = self.config_dir().join("bookmarks");
        std::fs::create_dir_all(&path)?;
        Bookmarks::open(path)
    }

    fn app_state_path(&self) -> PathBuf {
        self.state_dir().join("app_state.cbor")
    }

    pub fn load_app_state(&self) -> Result<AppSnapshot<AppState>, Error> {
        let path = self.app_state_path();
        tracing::debug!(path = %path.display(), "Loading app state");
        Ok(ciborium::from_reader(BufReader::new(File::open(path)?))?)
    }

    pub fn save_app_state(&self, snapshot: AppSnapshot<&AppState>) -> Result<(), Error> {
        let path = self.app_state_path();
        tracing::debug!(path = %path.display(), "Saving app state");
        ciborium::into_writer(&snapshot, BufWriter::new(File::create(path)?))?;
        Ok(())
    }

    pub fn log_file(&self) -> PathBuf {
        self.project_dirs.data_local_dir().join("mrrp-cli.log")
    }
}
