use crate::satnogs::{
    self,
    SatelliteId,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    TimeParse(#[from] chrono::ParseError),

    #[error(transparent)]
    Satnogs(#[from] satnogs::Error),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error("Satellite without TLE: {sat_id}")]
    NoTLE { sat_id: SatelliteId },

    #[error("Satellite not found: {sat_id}")]
    SatelliteNotFound { sat_id: SatelliteId },

    #[error(transparent)]
    SatkitSgp4(#[from] satkit::sgp4::Error),

    #[error(transparent)]
    SatkitUpdate(#[from] satkit::utils::update_data::Error),
}
