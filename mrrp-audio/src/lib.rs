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
#[cfg(feature = "wav")]
pub mod wav;

#[cfg(feature = "rodio")]
pub use rodio::play_audio;

#[cfg(feature = "wav")]
pub use crate::wav::{
    sink::{
        WavSink,
        write_stream_to_wav,
    },
    source::WavSource,
};
