use std::{
    fs::File,
    io::{
        BufWriter,
        Seek,
        Write,
    },
    marker::PhantomData,
    path::Path,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use num_complex::Complex;

use crate::io::{
    AsyncReadSamples,
    AsyncReadSamplesExt,
    AsyncWriteSamples,
    ForwardError,
    GetSampleRate,
};

#[derive(Debug, thiserror::Error)]
#[error("wav sink error")]
pub enum Error {
    Hound(#[from] hound::Error),
    Closed,
}

#[derive(derive_more::Debug)]
pub struct WavSink<W, S>
where
    W: Write + Seek,
{
    #[debug(skip)]
    inner: Option<hound::WavWriter<W>>,
    _phantom: PhantomData<fn(S)>,
}

impl<W, S> WavSink<W, S>
where
    W: Write + Seek,
{
    #[inline]
    pub fn new(inner: hound::WavWriter<W>) -> Self {
        Self {
            inner: Some(inner),
            _phantom: PhantomData,
        }
    }

    #[inline]
    fn writer_mut(&mut self) -> Result<&mut hound::WavWriter<W>, Error> {
        self.inner.as_mut().ok_or(Error::Closed)
    }
}

impl<W, S> WavSink<W, S>
where
    W: Write + Seek,
    S: IntoWavSamples,
{
    #[inline]
    pub fn from_writer(writer: W, sample_rate: f32) -> Result<Self, Error> {
        Ok(Self::new(hound::WavWriter::new(
            writer,
            S::spec(sample_rate as u32),
        )?))
    }
}

impl<S> WavSink<BufWriter<File>, S>
where
    S: IntoWavSamples,
{
    #[inline]
    pub fn from_path(path: impl AsRef<Path>, sample_rate: f32) -> Result<Self, Error> {
        Ok(Self::new(hound::WavWriter::create(
            path,
            S::spec(sample_rate as u32),
        )?))
    }
}

impl<W, S> AsyncWriteSamples<S> for WavSink<W, S>
where
    W: Write + Seek + Unpin,
    S: IntoWavSamples,
{
    type Error = Error;

    fn poll_write_samples(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        let writer = self.writer_mut()?;
        for sample in buffer {
            // todo: should we return the error even if we have written some samples? we
            // could return Ok(n) here if n > 0, otherwise Err(_).
            sample.write_samples(writer)?;
        }
        Poll::Ready(Ok(buffer.len()))
    }

    #[inline]
    fn poll_flush(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        self.writer_mut()?.flush()?;
        Poll::Ready(Ok(()))
    }

    #[inline]
    fn poll_close(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<(), Self::Error>> {
        if let Some(writer) = self.inner.take() {
            writer.finalize()?;
        }
        Poll::Ready(Ok(()))
    }
}

pub trait IntoWavSamples {
    fn spec(sample_rate: u32) -> hound::WavSpec;

    fn write_samples<W>(&self, writer: &mut hound::WavWriter<W>) -> Result<(), Error>
    where
        W: Write + Seek;
}

macro_rules! impl_into_wav_samples {
    {$(($T:ty, $bits:expr, $format:expr);)*} => {
        $(
            impl IntoWavSamples for $T {
                #[inline]
                fn spec(sample_rate: u32) -> hound::WavSpec {
                    hound::WavSpec {
                        channels: 1,
                        sample_rate,
                        bits_per_sample: $bits,
                        sample_format: $format,
                    }
                }

                #[inline]
                fn write_samples<W>(&self, writer: &mut hound::WavWriter<W>) -> Result<(), Error>
                where
                    W: Write + Seek,
                {
                    writer.write_sample(*self)?;
                    Ok(())
                }
            }

            impl IntoWavSamples for Complex<$T> {
                #[inline]
                fn spec(sample_rate: u32) -> hound::WavSpec {
                    hound::WavSpec {
                        channels: 2,
                        sample_rate,
                        bits_per_sample: $bits,
                        sample_format: $format,
                    }
                }

                #[inline]
                fn write_samples<W>(&self, writer: &mut hound::WavWriter<W>) -> Result<(), Error>
                where
                    W: Write + Seek,
                {
                    writer.write_sample(self.re)?;
                    writer.write_sample(self.im)?;
                    Ok(())
                }
            }
        )*
    };
}

impl_into_wav_samples! {
    (i8, 8, hound::SampleFormat::Int);
    (i16, 16, hound::SampleFormat::Int);
    (i32, 32, hound::SampleFormat::Int);
    (f32, 32, hound::SampleFormat::Float);
}

pub async fn write_stream_to_wav<R, S>(
    path: impl AsRef<Path>,
    source: R,
) -> Result<(), ForwardError<R::Error, Error>>
where
    R: AsyncReadSamples<S> + GetSampleRate,
    S: IntoWavSamples,
{
    let sink =
        WavSink::<_, S>::from_path(path, source.sample_rate()).map_err(ForwardError::Sink)?;
    source.forward(sink, 0x4000).await?;
    Ok(())
}
