use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use futures_util::{
    FutureExt,
    future::{
        Fuse,
        FusedFuture,
    },
    ready,
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
use tokio::time::Sleep;

use crate::signal::{
    AsyncReadSamples,
    ReadBuf,
};

pin_project! {
    #[derive(Debug)]
    pub struct Throttled<R> {
        #[pin]
        inner: R,
        sample_duration: Duration,
        delay: Pin<Box<Fuse<tokio::time::Sleep>>>,
    }
}

impl<R> Throttled<R> {
    pub fn new(inner: R, sample_duration: Duration) -> Self {
        Self {
            inner,
            sample_duration,
            delay: Box::pin(Fuse::terminated()),
        }
    }

    #[inline]
    pub fn into_inner(self) -> R {
        self.inner
    }

    #[inline]
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    #[inline]
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }
}

impl<R> Clone for Throttled<R>
where
    R: Clone,
{
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            sample_duration: self.sample_duration,
            // fixme: this unfortunately doesn't work, as Fuse doesn't let us
            // access the inner type
            // delay: Box::pin(tokio::time::sleep_until(self.delay.deadline()).
            // fuse()),
            delay: Box::pin(Fuse::terminated()),
        }
    }
}

impl<R> AsyncReadSamples for Throttled<R>
where
    R: AsyncReadSamples,
{
    type Sample = R::Sample;
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        loop {
            if this.delay.is_terminated() {
                // this branch always returns

                let num_samples_before = buffer.filled().len();
                match this.inner.poll_read_samples(cx, buffer) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Ready(Ok(())) => {
                        let num_samples = buffer.filled().len() - num_samples_before;
                        this.delay
                            .set(make_sleep(num_samples, *this.sample_duration));

                        return Poll::Ready(Ok(()));
                    }
                }
            }
            else {
                // either we return Poll::Pending or the future will be
                // terminated in the next loop iteration

                ready!(this.delay.poll_unpin(cx));
            }
        }
    }
}

impl<R> GetSampleRate for Throttled<R>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R> StreamLength for Throttled<R>
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

impl<R> FiniteStream for Throttled<R> where R: FiniteStream {}

impl<W, S> AsyncWriteSamples<S> for Throttled<W>
where
    W: AsyncWriteSamples<S>,
{
    type Error = W::Error;

    fn poll_write_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        let this = self.project();

        loop {
            if this.delay.is_terminated() {
                // this branch always returns

                match this.inner.poll_write_samples(cx, buffer) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Ready(Ok(num_samples)) => {
                        this.delay
                            .set(make_sleep(num_samples, *this.sample_duration));
                        return Poll::Ready(Ok(num_samples));
                    }
                }
            }
            else {
                // either we return Poll::Pending or the future will be
                // terminated in the next loop iteration

                ready!(this.delay.poll_unpin(cx));
            }
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_flush(cx)
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_close(cx)
    }
}

fn make_sleep(num_samples: usize, sample_duration: Duration) -> Fuse<Sleep> {
    // 32bit at 2.4 MHz is exhausted in about 30 minutes
    let num_samples = u32::try_from(num_samples).unwrap_or_else(|_| {
        tracing::warn!("samples per read is overflowing u32 in Throttled");
        u32::MAX
    });

    // well, we'll just wait 1 second... close enough :D
    let delay = sample_duration
        .checked_mul(num_samples)
        .unwrap_or_else(|| Duration::from_secs(1));

    tokio::time::sleep(delay).fuse()
}
