mod sine;

use std::{
    convert::Infallible,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::Stream;
pub use sine::{
    ComplexSineWave,
    SineWave,
};

use crate::{
    buf::SamplesMut,
    io::{
        AsyncReadSamples,
        ReadBuf,
    },
};

pub trait SignalGenerator {
    type Sample;

    fn set_sample_rate(&mut self, sample_rate: f32);
    fn next(&mut self) -> Self::Sample;

    #[inline]
    fn into_chunk_stream(self, chunk_size: usize) -> SignalGeneratorChunkStream<Self>
    where
        Self: Sized,
    {
        SignalGeneratorChunkStream {
            signal_generator: self,
            chunk_size,
        }
    }

    #[inline]
    fn into_read_samples(self) -> SignalGeneratorReadSamples<Self>
    where
        Self: Sized,
    {
        SignalGeneratorReadSamples {
            signal_generator: self,
        }
    }
}

impl<G: SignalGenerator> SignalGenerator for &mut G {
    type Sample = G::Sample;

    fn set_sample_rate(&mut self, sample_rate: f32) {
        (*self).set_sample_rate(sample_rate);
    }

    fn next(&mut self) -> Self::Sample {
        (*self).next()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SignalGeneratorChunkStream<G> {
    pub signal_generator: G,
    pub chunk_size: usize,
}

impl<G> Stream for SignalGeneratorChunkStream<G>
where
    G: SignalGenerator + Unpin,
{
    type Item = SamplesMut<G::Sample>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let chunk = SamplesMut::from_fn(self.chunk_size, || self.signal_generator.next());
        Poll::Ready(Some(chunk))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SignalGeneratorReadSamples<G> {
    pub signal_generator: G,
}

impl<G> AsyncReadSamples<G::Sample> for SignalGeneratorReadSamples<G>
where
    G: SignalGenerator + Unpin,
{
    type Error = Infallible;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<G::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        buffer.fill_with(|| self.signal_generator.next());
        Poll::Ready(Ok(()))
    }
}
