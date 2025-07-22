use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytemuck::Pod;
use futures_util::{
    Stream,
    StreamExt,
};
use num_complex::Complex;
use rtlsdr_async::{
    Chunk,
    Gain,
    Iq,
    RtlSdr,
    Samples,
};

use crate::{
    GetCenterFrequency,
    GetSampleRate,
    buf::{
        SampleBuf,
        SampleBufMut,
        TryAdvanceError,
    },
    io::{
        AsyncReadSamples,
        ReadBuf,
    },
    sample::IntoSample,
};

pub type Error = rtlsdr_async::Error;

#[inline]
fn try_advance_chunk<S>(chunk: &mut Chunk<S>, amount: usize) -> Result<(), TryAdvanceError> {
    if amount <= chunk.len() {
        chunk.slice(amount..);
        Ok(())
    }
    else {
        Err(TryAdvanceError {
            requested: amount,
            available: chunk.len(),
        })
    }
}

impl<S: Pod> SampleBuf<S> for Chunk<S> {
    #[inline]
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError> {
        try_advance_chunk(self, amount)
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk(&self) -> &[S] {
        self.samples()
    }
}

#[inline]
pub(crate) const fn convert_iq_to_complex(iq: Iq) -> Complex<u8> {
    Complex { re: iq.i, im: iq.q }
}

#[inline]
pub(crate) const fn convert_complex_to_iq(complex: Complex<u8>) -> Iq {
    Iq {
        i: complex.re,
        q: complex.im,
    }
}

#[derive(Clone, Debug)]
pub struct RtlSdrSource {
    #[allow(unused)]
    device: RtlSdr,
    stream: Samples<Iq>,
    chunk: Option<Chunk<Iq>>,
    sample_rate: u32,
    tuner_frequency: u32,
}

impl RtlSdrSource {
    pub async fn from_device(device: RtlSdr) -> Result<Self, Error> {
        let sample_rate = device.get_sample_rate().await?;
        let tuner_frequency = device.get_center_frequency().await?;
        let stream = device.samples().await?;
        Ok(Self {
            device,
            stream,
            chunk: None,
            sample_rate,
            tuner_frequency,
        })
    }

    pub async fn open(device: u32, tuner_frequency: f32, sample_rate: f32) -> Result<Self, Error> {
        let sample_rate = sample_rate as u32;
        let tuner_frequency = tuner_frequency as u32;

        let device = RtlSdr::open(device)?;
        device.set_sample_rate(sample_rate).await?;
        device.set_center_frequency(tuner_frequency).await?;
        device.set_tuner_gain(Gain::Auto).await?;
        let stream = device.samples().await?;

        Ok(Self {
            device,
            stream,
            chunk: None,
            sample_rate,
            tuner_frequency,
        })
    }
}

impl AsyncReadSamples<Complex<f32>> for RtlSdrSource {
    type Error = Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Complex<f32>>,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            let this = &mut *self;

            if let Some(chunk) = &mut this.chunk {
                while chunk.has_remaining() && buffer.has_remaining_mut() {
                    buffer.put_sample(convert_iq_to_complex(chunk.get_sample()).into_sample());
                }

                if !chunk.has_remaining() {
                    this.chunk = None;
                }

                return Poll::Ready(Ok(()));
            }
            else {
                match this.stream.poll_next_unpin(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => return Poll::Ready(Ok(())),
                    Poll::Ready(Some(Err(error))) => return Poll::Ready(Err(error)),
                    Poll::Ready(Some(Ok(chunk))) => {
                        this.chunk = Some(chunk);
                    }
                }
            }
        }
    }
}

impl Stream for RtlSdrSource {
    type Item = Result<Chunk<Iq>, Error>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = &mut *self;

        if let Some(chunk) = this.chunk.take() {
            Poll::Ready(Some(Ok(chunk)))
        }
        else {
            this.stream.poll_next_unpin(cx)
        }
    }
}

impl GetSampleRate for RtlSdrSource {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.sample_rate as f32
    }
}

impl GetCenterFrequency for RtlSdrSource {
    #[inline]
    fn center_frequency(&self) -> f32 {
        self.tuner_frequency as f32
    }
}
