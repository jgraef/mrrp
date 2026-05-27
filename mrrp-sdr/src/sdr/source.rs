use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use anyhow::Error;
use mrrp::{
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        ReadBuf,
        StreamLength,
        combinators::Throttled,
    },
    source::Noise,
};
use num_complex::ComplexDistribution;
use rand::rngs::SmallRng;
use rand_distr::Normal;

use crate::sdr::Iq;

// todo: should we put the Send + 'static bound on this?
// and should this maybe have a Debug bound?
pub trait Source: AsyncReadSamples<Iq, Error = Error> {
    fn info(&self) -> SourceInfo;

    // todo
}

#[derive(Clone, Copy, Debug)]
pub struct SourceInfo {
    pub center_frequency: u64,
    pub sample_rate: u64,
}

#[derive(Clone, Debug)]
pub struct MockSource {
    noise: Throttled<Noise<SmallRng, ComplexDistribution<Normal<f32>, Normal<f32>>>>,
    pub info: SourceInfo,
}

impl MockSource {
    pub fn new(info: SourceInfo) -> Self {
        let std_dev = 0.02;
        let distribution = Normal::new(0.0, std_dev).unwrap();
        let distribution = ComplexDistribution::new(distribution, distribution);

        let sample_time = Duration::from_secs_f32(1.0 / info.sample_rate as f32);

        Self {
            noise: Noise::new(rand::make_rng(), distribution).throttle(sample_time),
            info,
        }
    }
}

impl AsyncReadSamples<Iq> for MockSource {
    type Error = Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Iq>,
    ) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.noise)
            .poll_read_samples(cx, buffer)
            .map_err(|error| match error {})
    }
}

impl Source for MockSource {
    fn info(&self) -> SourceInfo {
        self.info
    }
}

impl StreamLength for MockSource {
    fn remaining(&self) -> mrrp::io::Remaining {
        mrrp::io::Remaining::Infinite
    }
}
