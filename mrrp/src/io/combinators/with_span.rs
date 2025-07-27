use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use pin_project_lite::pin_project;
use tracing::Span;

use crate::io::{
    AsyncReadSamples,
    AsyncWriteSamples,
    GetSampleRate,
    ReadBuf,
    StreamLength,
};

pin_project! {
    #[derive(Clone, Debug)]
    pub struct WithSpan<T> {
        #[pin]
        inner: T,
        span: Span,
    }
}

impl<T> WithSpan<T> {
    #[inline]
    pub fn new(inner: T, span: Span) -> Self {
        Self { inner, span }
    }
}

impl<T, S> AsyncReadSamples<S> for WithSpan<T>
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
        let this = self.project();
        let _guard = this.span.enter();
        this.inner.poll_read_samples(cx, buffer)
    }
}

impl<T, S> AsyncWriteSamples<S> for WithSpan<T>
where
    T: AsyncWriteSamples<S>,
{
    type Error = T::Error;

    #[inline]
    fn poll_write_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        let this = self.project();
        let _guard = this.span.enter();
        this.inner.poll_write_samples(cx, buffer)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        let _guard = this.span.enter();
        this.inner.poll_flush(cx)
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        let _guard = this.span.enter();
        this.inner.poll_close(cx)
    }
}

impl<T> GetSampleRate for WithSpan<T>
where
    T: GetSampleRate,
{
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<T> StreamLength for WithSpan<T>
where
    T: StreamLength,
{
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}
