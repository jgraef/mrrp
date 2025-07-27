use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use pin_project_lite::pin_project;

use crate::io::{
    AsyncReadSamples,
    FiniteStream,
    GetSampleRate,
    ReadBuf,
    Remaining,
    StreamLength,
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

impl<R, S, E, F> AsyncReadSamples<S> for MapErr<R, F>
where
    R: AsyncReadSamples<S>,
    F: FnMut(R::Error) -> E,
    E: std::error::Error,
{
    type Error = E;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
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
