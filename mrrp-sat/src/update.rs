use std::path::{
    Path,
    PathBuf,
};

use anyhow::Error;
use chrono::{
    DateTime,
    TimeDelta,
    Utc,
};

use crate::satellite::SatelliteDatabase;

#[derive(Debug)]
pub struct Updater {
    last_update_time_path: PathBuf,
    min_update_interval: TimeDelta,
}

impl Updater {
    pub fn new(data_dir: impl AsRef<Path>) -> Self {
        let last_update_time_path = data_dir.as_ref().join("last_update.txt");

        Self {
            last_update_time_path,
            // todo: make this configurable
            min_update_interval: TimeDelta::days(1),
        }
    }

    pub fn get_last_update_time(&self) -> Result<Option<DateTime<Utc>>, Error> {
        if self.last_update_time_path.exists() {
            Ok(Some(
                std::fs::read_to_string(&self.last_update_time_path)?.parse()?,
            ))
        }
        else {
            Ok(None)
        }
    }

    pub fn write_last_update_time(&self) -> Result<(), Error> {
        std::fs::write(&self.last_update_time_path, Utc::now().to_string())?;
        Ok(())
    }

    pub async fn perform_update(&self, satellites: &mut SatelliteDatabase) -> Result<(), Error> {
        tracing::info!("Updating satkit data");
        // todo: disabled during early dev, because we need to debug the update
        // mechanism a bit more. satkit_update().await?;

        tracing::info!("Updating satellite list");
        satellites.update().await?;

        self.write_last_update_time()?;

        Ok(())
    }

    pub async fn perform_auto_update(
        &self,
        satellites: &mut SatelliteDatabase,
    ) -> Result<(), Error> {
        let last_update_time = self.get_last_update_time()?;

        let needs_update = last_update_time.is_none_or(|last_update_time| {
            last_update_time + self.min_update_interval < Utc::now()
        });

        if needs_update {
            tracing::info!(?last_update_time, "Performing automatic update");
            self.perform_update(satellites).await?;
        }

        Ok(())
    }
}

async fn satkit_update() -> Result<(), Error> {
    tokio::task::spawn_blocking(|| satkit::utils::update_datafiles(None, false))
        .await
        .unwrap()
}
