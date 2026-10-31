use std::fmt::Debug;

use mrrp_core::{
    sample::Complex,
    signal::AsyncWriteSamples,
};

pub type SinkError = Box<dyn std::error::Error + Send + Sync + 'static>;

pub trait Sink:
    AsyncWriteSamples<Complex<f32>, Error = SinkError> + Debug + Send + Sync + 'static
{
}

impl<T> Sink for T where
    T: AsyncWriteSamples<Complex<f32>, Error = SinkError> + Debug + Send + Sync + 'static
{
}
