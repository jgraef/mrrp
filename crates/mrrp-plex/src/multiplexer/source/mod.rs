use std::{
    fmt::Debug,
    pin::Pin,
};

use mrrp_core::{
    sample::Complex,
    signal::{
        AsyncReadSamples,
        GetSampleRate,
    },
};

#[cfg(feature = "rtl-sdr")]
mod rtl_sdr;

#[cfg(feature = "rtl-sdr")]
pub use rtl_sdr::RtlSdr;

pub type SourceError = Box<dyn std::error::Error + Send + Sync + 'static>;

// todo: add `Seek + Pause`, and maybe `GetSampleRate`, etc.
pub trait Source:
    AsyncReadSamples<Sample = Complex<f32>, Error = SourceError>
    + GetSampleRate
    + Debug
    + Send
    + Sync
    + Unpin
    + 'static
{
    fn pause(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>>;
    fn resume(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>>;
}

pub trait IntoSource {
    fn into_source(self) -> Box<dyn Source>;
}

impl<T> IntoSource for T
where
    T: Source,
{
    fn into_source(self) -> Box<dyn Source> {
        Box::new(self)
    }
}
