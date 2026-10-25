use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
        ready,
    },
};

use mrrp_core::signal::{
    FiniteStream,
    GetSampleRate,
    Remaining,
    SizeHint,
    StreamLength,
};
use pin_project_lite::pin_project;
use tokio::task::coop;

use crate::signal::{
    AsyncReadSamples,
    ReadBuf,
};

pin_project! {
    #[derive(Clone, Copy, Debug)]
    pub struct Cooperative<T> {
        #[pin]
        inner: T,
    }
}

impl<T> Cooperative<T> {
    #[inline]
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> GetSampleRate for Cooperative<T>
where
    T: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<T> StreamLength for Cooperative<T>
where
    T: StreamLength,
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

impl<T> FiniteStream for Cooperative<T> where T: FiniteStream {}

impl<T> AsyncReadSamples for Cooperative<T>
where
    T: AsyncReadSamples,
{
    type Sample = T::Sample;
    type Error = T::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let coop = ready!(coop::poll_proceed(cx));

        self.project()
            .inner
            .poll_read_samples(cx, buffer)
            .map(|result| {
                coop.made_progress();
                result
            })
    }
}
