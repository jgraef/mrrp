use std::{
    path::Path,
    time::Duration,
};

use anyhow::Error;
use chrono::{
    DateTime,
    Utc,
};
use mrrp_audio::{
    WavSource,
    write_stream_to_wav,
};
use mrrp_core::{
    sample::Complex,
    signal::{
        FiniteStream,
        GetSampleRate,
    },
};
use mrrp_sat::{
    Geodetic,
    doppler_correct::DopplerCorrection,
    satellite::{
        ReferenceState,
        Satellite,
    },
};
use mrrp_util::signal::AsyncReadSamplesExt;

// todo: also take batch_size option and don't predict all satellite positions
// at once, but batch them.
pub async fn correct(
    satellite: &Satellite,
    location: Geodetic,
    nominal_frequency: f32,
    start_time: DateTime<Utc>,
    predict_interval: Duration,
    input: impl AsRef<Path>,
    output: impl AsRef<Path>,
) -> Result<(), Error> {
    let input = input.as_ref();
    let output = output.as_ref();

    tracing::debug!(sat=%satellite.id(), tle=?satellite.most_recent_tle(), "Correcting doppler");

    // open source file
    let source = WavSource::<_, Complex<i16>>::from_path(&input)?.convert::<Complex<f32>>();
    let duration = Duration::from_secs_f32(source.len() as f32 / source.sample_rate());

    let doppler_correction = DopplerCorrection::from_satellite(
        satellite,
        ReferenceState::from_geodetic(location),
        nominal_frequency,
        start_time,
        predict_interval,
        source.sample_rate(),
        duration,
    )?;

    // apply correction
    let corrected = source.scan_in_place_with(doppler_correction);

    // write corrected stream to file
    write_stream_to_wav(&output, corrected).await?;

    Ok(())
}
