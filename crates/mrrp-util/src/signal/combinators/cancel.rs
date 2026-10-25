use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::signal::{
    AsyncReadSamples,
    GetSampleRate,
    ReadBuf,
    Remaining,
    SizeHint,
    StreamLength,
};
use pin_project_lite::pin_project;
use tokio_util::sync::{
    CancellationToken,
    WaitForCancellationFutureOwned,
};

pin_project! {
    /// A stream that can be cancelled.
    ///
    /// This uses tokio's [`CancellationToken`] to signal the cancelation.
    pub struct Cancelable<R> {
        #[pin]
        inner: R,
        #[pin]
        cancelled: WaitForCancellationFutureOwned,
        token: CancellationToken,
    }
}

impl<R> Cancelable<R> {
    #[inline]
    pub fn new(inner: R) -> Self {
        let token = CancellationToken::new();
        Self::from_token(inner, token)
    }

    #[inline]
    pub fn from_token(inner: R, token: CancellationToken) -> Self {
        let cancelled = token.clone().cancelled_owned();
        Self {
            inner,
            cancelled,
            token,
        }
    }

    #[inline]
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    #[inline]
    pub fn cancel(&self) {
        self.token.cancel();
    }
}

impl<R> AsyncReadSamples for Cancelable<R>
where
    R: AsyncReadSamples,
{
    type Sample = R::Sample;
    type Error = R::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        if let Poll::Ready(()) = this.cancelled.poll(cx) {
            Poll::Ready(Ok(()))
        }
        else {
            this.inner.poll_read_samples(cx, buffer)
        }
    }
}

impl<R> StreamLength for Cancelable<R>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        SizeHint {
            lower_bound: 0,
            upper_bound: self.inner.size_hint().upper_bound,
        }
    }
}

impl<R> GetSampleRate for Cancelable<R>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}
