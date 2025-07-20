// should we make audio sinks? currently this just turns any stream into rodio
// sources.

use futures_util::FutureExt;
use parking_lot::Mutex;
use rodio::Source as _;

use crate::{
    GetSampleRate,
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        Buffered,
    },
};

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct RodioSource<R> {
    pub read_samples: Buffered<R, f32>,
}

impl<R> RodioSource<R>
where
    R: AsyncReadSamples<f32>,
{
    pub fn new(read_samples: R) -> Self {
        Self {
            read_samples: read_samples.buffered(0x4000),
        }
    }
}

impl<R> rodio::Source for RodioSource<R>
where
    R: AsyncReadSamples<f32> + GetSampleRate + Unpin,
{
    #[inline]
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    #[inline]
    fn channels(&self) -> rodio::ChannelCount {
        1
    }

    #[inline]
    fn sample_rate(&self) -> rodio::SampleRate {
        self.read_samples.sample_rate() as u32
    }

    #[inline]
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl<R> Iterator for RodioSource<R>
where
    R: AsyncReadSamples<f32> + Unpin,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        match self.read_samples.read_sample().now_or_never() {
            None => {
                //tracing::warn!("audio buffer underrun");
                Some(0.0)
            }
            Some(Err(error)) => {
                tracing::warn!(?error);
                None
            }
            Some(Ok(sample)) => Some(sample),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("audio error")]
pub enum Error {
    Stream(#[from] rodio::StreamError),
}

pub fn play_audio<S>(signal: S, volume: f32) -> Result<(), Error>
where
    S: AsyncReadSamples<f32> + GetSampleRate + Unpin + Send + 'static,
{
    let source = RodioSource::new(signal);
    //let source = rodio::source::SineWave::new(440.0);

    global_output_stream()?.mixer().add(
        source
            .automatic_gain_control(1.0, 4.0, 0.0, 5.0)
            .amplify_normalized(volume),
    );
    Ok(())
}

fn global_output_stream() -> Result<&'static rodio::OutputStream, Error> {
    static OUTPUT_STREAM: Mutex<Option<&'static rodio::OutputStream>> = Mutex::new(None);

    let mut output_stream = OUTPUT_STREAM.lock();

    if output_stream.is_none() {
        *output_stream = Some(Box::leak(Box::new(
            rodio::OutputStreamBuilder::open_default_stream()?,
        )));
    }

    Ok(output_stream.unwrap())
}
