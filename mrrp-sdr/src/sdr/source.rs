use std::{
    f32::consts::TAU,
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
use num_complex::Complex;
use num_traits::Float;
use rand::{
    Rng,
    rngs::SmallRng,
};
use rand_distr::{
    Distribution,
    Normal,
    Uniform,
};

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
    noise: Throttled<Noise<SmallRng, PolarDistribution<Normal<f32>, Uniform<f32>>>>,
    pub info: SourceInfo,
}

impl MockSource {
    pub fn new(info: SourceInfo) -> Self {
        Self {
            noise: Noise::new(
                rand::make_rng(),
                PolarDistribution {
                    amplitude: Normal::new(0.0, 0.00002).unwrap(),
                    phase: Uniform::new(0.0, TAU).unwrap(),
                },
            )
            .throttle(Duration::from_secs_f32(1.0 / info.sample_rate as f32)),
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

#[derive(Clone, Copy, Debug)]
pub struct PolarDistribution<A, P> {
    pub amplitude: A,
    pub phase: P,
}

impl<T, A, P> Distribution<Complex<T>> for PolarDistribution<A, P>
where
    T: Float,
    A: Distribution<T>,
    P: Distribution<T>,
{
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Complex<T> {
        Complex::from_polar(self.amplitude.sample(rng), self.phase.sample(rng))
    }
}
