use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::signal::{
    FiniteStream,
    GetSampleRate,
    Remaining,
    StreamLength,
};
use pin_project_lite::pin_project;

use crate::signal::{
    AsyncReadSamples,
    ReadBuf,
};

pin_project! {
    /// Stream wrapper that maps the error type.
    #[derive(Clone, Copy, Debug)]
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
}

impl<R, F> FiniteStream for MapErr<R, F> where R: FiniteStream {}
