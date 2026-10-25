use std::{
    pin::Pin,
    task::Poll,
    time::Instant,
};

use mrrp_core::{
    buf::SampleBufMut,
    signal::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
        Remaining,
        SizeHint,
        StreamLength,
    },
};
use pin_project_lite::pin_project;

use crate::signal::Scanner;

pin_project! {
    #[derive(Debug)]
    pub struct Progress<R, P> {
        #[pin]
        inner: R,
        on_progress: P,
    }
}

impl<R, P> Progress<R, P> {
    #[inline]
    pub fn new(inner: R, on_progress: P) -> Self {
        Self { inner, on_progress }
    }
}

impl<R, P> AsyncReadSamples for Progress<R, P>
where
    R: AsyncReadSamples,
    P: Scanner<usize, Output = ()>,
{
    type Sample = R::Sample;
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        let remaining_before = buffer.remaining_mut();

        let poll = this.inner.poll_read_samples(cx, buffer);

        let num_samples = remaining_before - buffer.remaining_mut();

        if num_samples > 0 {
            this.on_progress.scan(num_samples)
        }

        poll
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ProgressInfo {
    pub start_time: Instant,
    pub now: Instant,
    pub progress: usize,
    pub total: Option<usize>,
}

impl<R, P> StreamLength for Progress<R, P>
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

impl<R, P> GetSampleRate for Progress<R, P>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}
