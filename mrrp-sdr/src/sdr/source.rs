use std::{
    f32::consts::TAU,
    fs::File,
    io::BufReader,
    path::Path,
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
        GetSampleRate,
        ReadBuf,
        StreamLength,
        combinators::{
            Converted,
            Throttled,
        },
    },
    source::{
        Noise,
        file::WavSource,
    },
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
    pub center_frequency: f32,
    pub sample_rate: f32,
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

#[derive(Debug)]
pub struct LoopedFileSource {
    inner: Converted<WavSource<BufReader<File>, Complex<i16>>, Complex<i16>, Complex<f32>>,
    center_frequency: f32,
}

impl LoopedFileSource {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let inner = WavSource::from_path(path)?.convert::<Iq>();
        tracing::debug!(
            sample_rate = inner.inner().spec().sample_rate,
            "opened wav file"
        );

        Ok(Self {
            inner,
            center_frequency: 0.0,
        })
    }

    pub fn with_center_frequency(mut self, center_frequency: f32) -> Self {
        self.center_frequency = center_frequency;
        self
    }
}

impl AsyncReadSamples<Iq> for LoopedFileSource {
    type Error = Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Iq>,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            let before = buffer.filled().len();
            match Pin::new(&mut self.inner).poll_read_samples(cx, buffer) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error.into())),
                Poll::Ready(Ok(())) => {
                    if before == buffer.filled().len() {
                        // nothing was read, end of file
                        self.inner.inner_mut().seek(0)?;
                    }
                    else {
                        return Poll::Ready(Ok(()));
                    }
                }
            }
        }
    }
}

impl Source for LoopedFileSource {
    fn info(&self) -> SourceInfo {
        SourceInfo {
            center_frequency: self.center_frequency,
            sample_rate: self.inner.sample_rate(),
        }
    }
}

impl StreamLength for LoopedFileSource {
    fn remaining(&self) -> mrrp::io::Remaining {
        mrrp::io::Remaining::Infinite
    }
}
