use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::Stream;
use pin_project_lite::pin_project;

use crate::io::{
    AsyncReadSamples,
    GetSampleRate,
    ReadBuf,
};

pin_project! {
    #[derive(Clone, Copy, Debug)]
    pub struct WithSampleRate<T> {
        #[pin]
        inner: T,
        sample_rate: f32,
    }
}

impl<T> WithSampleRate<T> {
    pub fn new(inner: T, sample_rate: f32) -> Self {
        Self { inner, sample_rate }
    }
}

impl<T> GetSampleRate for WithSampleRate<T> {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

impl<T, S> AsyncReadSamples<S> for WithSampleRate<T>
where
    T: AsyncReadSamples<S>,
{
    type Error = T::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_read_samples(cx, buffer)
    }
}

impl<T> Stream for WithSampleRate<T>
where
    T: Stream,
{
    type Item = T::Item;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}
