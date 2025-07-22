use std::{
    fs::File,
    io::BufReader,
    marker::PhantomData,
    path::Path,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use num_complex::Complex;

use crate::{
    GetSampleRate,
    buf::SampleBufMut,
    io::{
        AsyncReadSamples,
        ReadBuf,
    },
};

#[derive(Debug, thiserror::Error)]
#[error("wav source error")]
pub enum Error {
    Hound(#[from] hound::Error),
    UnexpectedChannelCount {
        channels: u16,
        expected: u16,
    },
    UnexpectedBitsPerSample {
        bits_per_sample: u16,
        expected: u16,
    },
    UnexpectedSampleFormat {
        sample_format: hound::SampleFormat,
        expected: hound::SampleFormat,
    },
}

#[derive(derive_more::Debug)]
pub struct WavSource<R, S> {
    #[debug(skip)]
    inner: hound::WavReader<R>,
    spec: hound::WavSpec,
    _phantom: PhantomData<fn() -> S>,
}

impl<R, S> WavSource<R, S>
where
    R: std::io::Read,
    S: FromWavSamples,
{
    pub fn new(inner: hound::WavReader<R>) -> Result<Self, Error> {
        let spec = inner.spec();
        S::check_spec(&spec)?;
        Ok(Self {
            inner,
            spec,
            _phantom: PhantomData,
        })
    }

    #[inline]
    pub fn from_reader(reader: R) -> Result<Self, Error> {
        Self::new(hound::WavReader::new(reader)?)
    }
}

impl<S> WavSource<BufReader<File>, S>
where
    S: FromWavSamples,
{
    #[inline]
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::new(hound::WavReader::open(path)?)
    }
}

impl<R, S> AsyncReadSamples<S> for WavSource<R, S>
where
    R: std::io::Read + Unpin,
    S: FromWavSamples,
{
    type Error = Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let mut samples = self.inner.samples();

        while buffer.has_remaining_mut() {
            let Some(sample) = S::from_samples(&mut samples)?
            else {
                break;
            };
            buffer.put_sample(sample);
        }

        Poll::Ready(Ok(()))
    }
}

impl<R, S> GetSampleRate for WavSource<R, S> {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.spec.sample_rate as f32
    }
}

pub trait FromWavSamples: Sized {
    type WavSample: hound::Sample;

    fn check_spec(spec: &hound::WavSpec) -> Result<(), Error>;
    fn from_samples<'a, R>(
        samples: &mut hound::WavSamples<'a, R, Self::WavSample>,
    ) -> Result<Option<Self>, Error>
    where
        R: std::io::Read;
}

macro_rules! impl_from_wav_samples {
    {$(($T:ty, $bits:expr, $format:expr);)*} => {
        $(
            impl FromWavSamples for $T {
                type WavSample = $T;

                #[inline]
                fn check_spec(spec: &hound::WavSpec) -> Result<(), Error> {
                    spec_expect_channels(spec, 1)?;
                    spec_expect_bits_per_sample(spec, $bits)?;
                    spec_expect_sample_format(spec, $format)?;
                    Ok(())
                }

                #[inline]
                fn from_samples<'a, R>(
                    samples: &mut hound::WavSamples<'a, R, Self::WavSample>,
                ) -> Result<Option<Self>, Error>
                where
                    R: std::io::Read,
                {
                    samples.next().transpose().map_err(Into::into)
                }
            }

            impl FromWavSamples for Complex<$T> {
                type WavSample = $T;

                #[inline]
                fn check_spec(spec: &hound::WavSpec) -> Result<(), Error> {
                    spec_expect_channels(spec, 2)?;
                    spec_expect_bits_per_sample(spec, $bits)?;
                    spec_expect_sample_format(spec, $format)?;
                    Ok(())
                }

                #[inline]
                fn from_samples<'a, R>(
                    samples: &mut hound::WavSamples<'a, R, Self::WavSample>,
                ) -> Result<Option<Self>, Error>
                where
                    R: std::io::Read,
                {
                    let re_opt = samples.next().transpose()?;
                    let im_opt = samples.next().transpose()?;
                    Ok(re_opt.zip(im_opt).map(|(re, im)| Complex { re, im }))
                }
            }
        )*
    };
}

impl_from_wav_samples! {
    (i8, 8, hound::SampleFormat::Int);
    (i16, 16, hound::SampleFormat::Int);
    (i32, 32, hound::SampleFormat::Int);
    (f32, 32, hound::SampleFormat::Float);
}

#[inline]
fn spec_expect_channels(spec: &hound::WavSpec, expected: u16) -> Result<(), Error> {
    if spec.channels == expected {
        Ok(())
    }
    else {
        Err(Error::UnexpectedChannelCount {
            channels: spec.channels,
            expected,
        })
    }
}

#[inline]
fn spec_expect_bits_per_sample(spec: &hound::WavSpec, expected: u16) -> Result<(), Error> {
    if spec.bits_per_sample == expected {
        Ok(())
    }
    else {
        Err(Error::UnexpectedBitsPerSample {
            bits_per_sample: spec.bits_per_sample,
            expected,
        })
    }
}

#[inline]
fn spec_expect_sample_format(
    spec: &hound::WavSpec,
    expected: hound::SampleFormat,
) -> Result<(), Error> {
    if spec.sample_format == expected {
        Ok(())
    }
    else {
        Err(Error::UnexpectedSampleFormat {
            sample_format: spec.sample_format,
            expected,
        })
    }
}
