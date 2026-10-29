use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::signal::{
    AsyncWriteSamples,
    FiniteStream,
    GetSampleRate,
    Remaining,
    SizeHint,
    StreamLength,
};
use pin_project_lite::pin_project;

use crate::{
    signal::{
        AsyncReadSamples,
        ReadBuf,
    },
    util::SkipDebug,
};

pin_project! {
    /// Stream wrapper that maps the error type.
    #[derive(Clone, Copy)]
    pub struct MapErr<R, F> {
        #[pin]
        inner: R,
        map_err: F,
    }
}

impl<R, F> MapErr<R, F> {
    #[inline]
    pub fn new(inner: R, map_err: F) -> Self {
        Self { inner, map_err }
    }
}

impl<R, F> Debug for MapErr<R, F>
where
    R: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MapErr")
            .field("inner", &self.inner)
            .field("map_err", &SkipDebug::closure(&self.map_err))
            .finish()
    }
}

impl<R, E, F> AsyncReadSamples for MapErr<R, F>
where
    R: AsyncReadSamples,
    F: FnMut(R::Error) -> E,
{
    type Sample = R::Sample;
    type Error = E;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.inner
            .poll_read_samples(cx, buffer)
            .map_err(this.map_err)
    }
}

impl<R, F> GetSampleRate for MapErr<R, F>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, F> StreamLength for MapErr<R, F>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl<R, F> FiniteStream for MapErr<R, F> where R: FiniteStream {}

impl<W, S, E, F> AsyncWriteSamples<S> for MapErr<W, F>
where
    W: AsyncWriteSamples<S>,
    F: FnMut(W::Error) -> E,
{
    type Error = E;

    #[inline]
    fn poll_write_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        let this = self.project();
        this.inner
            .poll_write_samples(cx, buffer)
            .map_err(this.map_err)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.inner.poll_flush(cx).map_err(this.map_err)
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.inner.poll_close(cx).map_err(this.map_err)
    }
}
