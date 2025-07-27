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
use pin_project_lite::pin_project;

use crate::io::{
    AsyncReadSamples,
    GetSampleRate,
    ReadBuf,
    StreamLength,
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
}

impl<R, S> AsyncReadSamples<S> for Throttled<R>
where
    R: AsyncReadSamples<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
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

                        // 32bit at 2.4 MHz is exhausted in about 30 minutes
                        let num_samples = u32::try_from(num_samples).unwrap_or_else(|_| {
                            tracing::warn!("samples per read is overflowing u32 in Throttled");
                            u32::MAX
                        });

                        // well, we'll just wait 1 second... close enough :D
                        let delay = this
                            .sample_duration
                            .checked_mul(num_samples)
                            .unwrap_or_else(|| Duration::from_secs(1));

                        this.delay.set(tokio::time::sleep(delay).fuse());

                        return Poll::Ready(Ok(()));
                    }
                }
            }
            else {
                // either we return Poll::Pending or the future will be terminated in the next
                // loop iteration

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
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}
