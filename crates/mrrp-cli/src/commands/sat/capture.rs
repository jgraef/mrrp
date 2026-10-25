use std::path::Path;

use anyhow::Error;
use chrono::{
    TimeDelta,
    Utc,
};
use mrrp_audio::write_stream_to_wav;
use mrrp_core::sample::Complex;
use mrrp_rtl_sdr::Device;
use mrrp_sat::{
    Geodetic,
    pass::{
        PassEvent,
        PassEvents,
        PassEventsOptions,
    },
    satellite::Satellite,
};
use mrrp_util::signal::AsyncReadSamplesExt;

use crate::{
    commands::sat::PassOptions,
    util::{
        SourceInfo,
        sleep_until,
    },
};

#[derive(Clone, Debug, Default, clap::Args)]
pub struct Options {
    #[clap(flatten)]
    pub pass_options: PassOptions,

    /// Ignores when the next pass is actually going to happen and makes it
    /// happen in 15s. Each pass will be 15s long. This can be used to test the
    /// whole pipeline without waiting for the satellite to pass.
    #[clap(long)]
    pub test_time: bool,
}

pub async fn capture(
    output: impl AsRef<Path>,
    satellite: &Satellite,
    location: Geodetic,
    options: Options,
    device: &mut Device,
    center_frequency: f32,
) -> Result<(), Error> {
    let output = output.as_ref();

    let passes = PassEvents::new(
        satellite,
        location,
        PassEventsOptions {
            start_time: None,
            predict_interval: options.pass_options.predict.interval,
            batch_size: options.pass_options.predict.batch_size,
            min_elevation: options.pass_options.min_elevation,
        },
    );

    for result in passes {
        match result {
            Ok(event) => {
                match event {
                    PassEvent::End(complete_pass) => {
                        let (start, end) = if options.test_time {
                            tracing::info!("Using test time.");

                            let now = Utc::now();
                            let start = now + TimeDelta::seconds(15);
                            let end = now + TimeDelta::seconds(30);
                            (start, end)
                        }
                        else {
                            let start = complete_pass.start.state.time();
                            let end = complete_pass.end.state.time();
                            (start, end)
                        };

                        tracing::debug!(%start, "sleeping until pass starts");
                        sleep_until(start).await;

                        // note: filename format expected by SDR++ so that it
                        // can determine center frequency
                        let file_name = SourceInfo {
                            start_time: start,
                            frequency: center_frequency,
                        }
                        .filename("baseband", "wav");
                        let path = output.join(&file_name);
                        tracing::info!(%file_name, "Pass started");

                        let reader = device.reader(Default::default()).await?;

                        // note: we convert to i16 even though i8 would be
                        // sufficient, so that SDR++ can read it. SDR++ can
                        // somewhat read i8, but it doesn't look right.
                        let stream = reader.convert::<Complex<i16>>().cancelable();

                        let stop = stream.token();

                        let handle = tokio::spawn({
                            async move {
                                tracing::debug!("start recording");
                                let result = write_stream_to_wav(path, stream).await;
                                tracing::debug!(?result, "stop recording");
                                result
                            }
                        });

                        tracing::debug!(%end, "sleeping until pass ends");
                        sleep_until(end).await;
                        stop.cancel();

                        if let Err(error) = handle.await {
                            tracing::error!(%error, "error while capturing baseband");
                        }
                    }
                    _ => {}
                }
            }
            Err(error) => {
                tracing::error!(%error);
            }
        }
    }

    Ok(())
}
