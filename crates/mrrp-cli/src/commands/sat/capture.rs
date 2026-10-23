use std::{
    path::Path,
    time::Duration,
};

use anyhow::Error;
use chrono::{
    DateTime,
    Local,
    TimeDelta,
    Utc,
};
use mrrp_audio::write_stream_to_wav;
use mrrp_core::sample::Complex;
use mrrp_rtl_sdr::{
    Device,
    tuner::gain::IntoTunerGain,
};
use mrrp_sat::{
    Geodetic,
    pass::{
        PassEvent,
        PassEvents,
        PassEventsOptions,
    },
    satellite::Satellite,
};
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    Silence,
    Throttled,
    WithSampleRate,
};

use crate::util::{
    CancelStream,
    sleep_until,
};

#[derive(Clone, Debug, clap::Args)]
pub struct Options {
    #[clap(short, long, default_value = "5s", value_parser = humantime::parse_duration)]
    pub predict_interval: Duration,

    #[clap(short = 'b', long, default_value = "1024")]
    pub batch_size: usize,

    #[clap(
        short = 'e',
        long,
        default_value = "0",
        value_parser = crate::util::parse_degrees_as_radians::<f64>,
    )]
    pub min_elevation: f64,

    #[clap(long)]
    pub test_time: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            predict_interval: Duration::from_secs(5),
            batch_size: 1024,
            min_elevation: 0.0f64.to_radians(),
            test_time: false,
        }
    }
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
            predict_interval: options.predict_interval,
            batch_size: options.batch_size,
            min_elevation: options.min_elevation,
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
                        let file_name = format!(
                            "baseband_{}Hz_{}.wav",
                            center_frequency as u32,
                            DateTime::<Local>::from(start).format("%H-%M-%S_%d-%m-%Y")
                        );
                        let path = output.join(&file_name);
                        tracing::info!(%file_name, "Pass started");

                        let reader = device.reader(Default::default()).await?;

                        // note: we convert to i16 even though i8 would be
                        // sufficient, so that SDR++ can read it. SDR++ can
                        // somewhat read i8, but it doesn't look right.
                        let stream = CancelStream::new(reader.convert::<Complex<i16>>());

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

    todo!();
}

#[derive(Debug)]
pub struct DummyDevice {
    sample_rate: f32,
}

impl Default for DummyDevice {
    fn default() -> Self {
        Self { sample_rate: 1e6 }
    }
}

impl DummyDevice {
    pub async fn reader(&mut self, _options: ()) -> Result<DummyReader, !> {
        Ok(Silence::<Complex<u8>>::default()
            .with_sample_rate(self.sample_rate)
            .throttle_to_sample_rate())
    }

    pub async fn set_sample_rate(&mut self, sample_rate: f32) -> Result<(), !> {
        self.sample_rate = sample_rate;
        Ok(())
    }

    pub async fn set_center_frequency(&mut self, _center_frequency: f32) -> Result<(), !> {
        Ok(())
    }

    pub async fn set_bias_t(&mut self, _enable: bool) -> Result<(), !> {
        Ok(())
    }

    pub async fn set_tuner_gain(&mut self, _tuner_gain: impl IntoTunerGain) -> Result<(), !> {
        Ok(())
    }
}

pub type DummyReader = Throttled<WithSampleRate<Silence<Complex<u8>>>>;
