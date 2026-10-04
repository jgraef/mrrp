//! [Wav](https://en.wikipedia.org/wiki/WAV) as signal sources and sink.

pub mod sink;
pub mod source;

pub use crate::wav::{
    sink::{
        WavSink,
        write_stream_to_wav,
    },
    source::WavSource,
};
