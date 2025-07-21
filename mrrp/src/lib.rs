#![feature(allocator_api)]
#![feature(get_mut_unchecked)]
#![feature(maybe_uninit_write_slice)]
#![feature(maybe_uninit_slice)]

use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::{
    Stream,
    StreamExt,
};

use crate::io::{
    AsyncReadSamples,
    ReadBuf,
};

#[cfg(feature = "audio")]
pub mod audio;
pub mod buf;
pub mod chunk;
mod demod;
pub mod filter;
pub mod io;
pub mod sample;
pub mod sink;
pub mod source;
pub mod util;

pub trait GetSampleRate {
    fn sample_rate(&self) -> f32;
}

impl<T: GetSampleRate> GetSampleRate for &T {
    #[inline]
    fn sample_rate(&self) -> f32 {
        (&**self).sample_rate()
    }
}

impl<T: GetSampleRate> GetSampleRate for &mut T {
    #[inline]
    fn sample_rate(&self) -> f32 {
        (&**self).sample_rate()
    }
}

pub trait GetCenterFrequency {
    fn center_frequency(&self) -> f32;
}

impl<T: GetCenterFrequency> GetCenterFrequency for &T {
    #[inline]
    fn center_frequency(&self) -> f32 {
        (&**self).center_frequency()
    }
}

impl<T: GetCenterFrequency> GetCenterFrequency for &mut T {
    #[inline]
    fn center_frequency(&self) -> f32 {
        (&**self).center_frequency()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WithSampleRate<T> {
    pub inner: T,
    pub sample_rate: f32,
}

impl<T> GetSampleRate for WithSampleRate<T> {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

impl<T, S> AsyncReadSamples<S> for WithSampleRate<T>
where
    T: AsyncReadSamples<S> + Unpin,
{
    type Error = T::Error;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.inner).poll_read_samples(cx, buffer)
    }
}

impl<T> Stream for WithSampleRate<T>
where
    T: Stream + Unpin,
{
    type Item = T::Item;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.poll_next_unpin(cx)
    }
}
