pub mod rtl_sdr;

use std::{
    borrow::Cow,
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
    signal::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        GetSampleRate,
        ReadBuf,
        Remaining,
        StreamLength,
        combinators::{
            Converted,
            Throttled,
        },
    },
    source::Noise,
};
use mrrp_audio::WavSource;
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

pub trait IntoSource {
    type Source: Source;

    fn into_source(self) -> Self::Source;
}

pub trait Source: AsyncReadSamples<Iq, Error = Error> {
    fn name(&self) -> &str;
    fn center_frequency(&self) -> f32;
    fn sample_rate(&self) -> f32;

    /// Start the underlying sample stream
    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>>;

    /// Stop the underlying sample stream
    ///
    /// While stopped the AsyncReadSamples implementation of this may always
    /// return `Poll::Ready(Ok(()))` without filling the buffer, and thus
    /// indicate an EOF condition.
    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>>;

    // todo
}

impl<S> IntoSource for S
where
    S: Source,
{
    type Source = Self;

    fn into_source(self) -> Self::Source {
        self
    }
}

#[derive(Clone, Debug)]
pub struct MockSource {
    noise: Throttled<Noise<SmallRng, PolarDistribution<Normal<f32>, Uniform<f32>>>>,
    center_frequency: f32,
    sample_rate: f32,
    active: bool,
}

impl MockSource {
    pub fn new(center_frequency: f32, sample_rate: f32) -> Self {
        Self {
            noise: Noise::new(
                rand::make_rng(),
                PolarDistribution {
                    amplitude: Normal::new(0.0, 0.005).unwrap(),
                    phase: Uniform::new(0.0, TAU).unwrap(),
                },
            )
            .throttle(Duration::from_secs_f32(1.0 / sample_rate)),
            center_frequency,
            sample_rate,
            active: true, // todo
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
        if self.active {
            Pin::new(&mut self.noise)
                .poll_read_samples(cx, buffer)
                .map_err(|error| match error {})
        }
        else {
            Poll::Ready(Ok(()))
        }
    }
}

impl Source for MockSource {
    fn name(&self) -> &str {
        "Mock"
    }

    fn center_frequency(&self) -> f32 {
        self.center_frequency
    }

    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(async {
            self.active = true;
            Ok(())
        })
    }

    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(async {
            self.active = false;
            Ok(())
        })
    }
}

impl StreamLength for MockSource {
    fn remaining(&self) -> Remaining {
        if self.active {
            Remaining::Infinite
        }
        else {
            Remaining::Finite { num_samples: 0 }
        }
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
    inner:
        Throttled<Converted<WavSource<BufReader<File>, Complex<i16>>, Complex<i16>, Complex<f32>>>,
    center_frequency: f32,
    name: Cow<'static, str>,
    active: bool,
}

impl LoopedFileSource {
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Error> {
        let name = path
            .as_ref()
            .file_name()
            .and_then(|file_name| file_name.to_str())
            .map_or_else(
                || "Looped File".into(),
                |file_name| format!("Looped File {file_name}").into(),
            );

        let inner = WavSource::from_path(path)?
            .convert::<Iq>()
            .throttle_to_sample_rate();

        tracing::debug!(sample_rate = ?inner.sample_rate(), "opened wav file");

        Ok(Self {
            inner,
            center_frequency: 0.0,
            name,
            active: true, // todo
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
        if self.active {
            loop {
                let before = buffer.filled().len();
                match Pin::new(&mut self.inner).poll_read_samples(cx, buffer) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error.into())),
                    Poll::Ready(Ok(())) => {
                        if before == buffer.filled().len() {
                            // nothing was read, end of file
                            self.inner.inner_mut().inner_mut().seek(0)?;
                        }
                        else {
                            buffer.filled_mut().iter_mut().for_each(|v| {
                                *v *= 0.001;
                            });

                            return Poll::Ready(Ok(()));
                        }
                    }
                }
            }
        }
        else {
            Poll::Ready(Ok(()))
        }
    }
}

impl Source for LoopedFileSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn center_frequency(&self) -> f32 {
        self.center_frequency
    }

    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }

    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(async {
            self.active = true;
            Ok(())
        })
    }

    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(async {
            self.active = false;
            Ok(())
        })
    }
}

impl StreamLength for LoopedFileSource {
    fn remaining(&self) -> Remaining {
        if self.active {
            Remaining::Infinite
        }
        else {
            Remaining::Finite { num_samples: 0 }
        }
    }
}
