//! Audio integration for mrrp
//!
//! This crate contains sinks and sources for sound cards (using [rodio]) and
//! audio file formats.
//!
//! # Audio playback and capture
//!
//! TODO
//!
//! # Audio files
//!
//! Currently only [wav](https://en.wikipedia.org/wiki/WAV) is supported using
//! the [hound] crate. [symphonia](https://docs.rs/symphonia/latest/symphonia/)
//! is another great crate that offers support for many container formats and
//! codecs, but it lacks encoding support.

#[cfg(feature = "rodio")]
pub mod rodio;
use std::path::Path;

#[cfg(feature = "wav")]
use mrrp_core::signal::{
    AsyncReadSamples,
    GetSampleRate,
};
#[cfg(feature = "wav")]
use mrrp_format::wav::sink::IntoWavSamples;
#[cfg(feature = "wav")]
pub use mrrp_format::wav::{
    self,
    sink::WavSink,
    source::WavSource,
};
#[cfg(feature = "wav")]
use mrrp_util::signal::AsyncReadSamplesExt;
use mrrp_util::signal::ForwardError;
#[cfg(feature = "rodio")]
pub use rodio::play_audio;

/// Write a stream to file as WAV.
///
/// # Deprecation Notice
///
/// This will be deprecated at some point in the future.
#[cfg(feature = "wav")]
pub async fn write_stream_to_wav<R>(
    path: impl AsRef<Path>,
    source: R,
) -> Result<(), ForwardError<R::Error, wav::sink::Error>>
where
    R: AsyncReadSamples + GetSampleRate,
    R::Sample: IntoWavSamples,
{
    let sink = WavSink::<_, R::Sample>::from_path(path, source.sample_rate())
        .map_err(ForwardError::Sink)?;
    source.forward(sink, 0x4000).await?;
    Ok(())
}
