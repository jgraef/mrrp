use std::{
    mem::MaybeUninit,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Instant,
};

use futures_util::Stream;
use mrrp_core::{
    buf::{
        SampleBufMut,
        UninitSlice,
    },
    signal::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
        Remaining,
        SizeHint,
        StreamLength,
    },
};
use mrrp_util::signal::Scanner;
use pin_project_lite::pin_project;
use tokio_util::sync::{
    CancellationToken,
    WaitForCancellationFutureOwned,
};

pin_project! {
    pub struct CancelStream<R> {
        #[pin]
        inner: R,
        #[pin]
        cancelled: WaitForCancellationFutureOwned,
        token: CancellationToken,
    }
}

impl<R> CancelStream<R> {
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

impl<R> AsyncReadSamples for CancelStream<R>
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

impl<R> StreamLength for CancelStream<R>
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

impl<R> GetSampleRate for CancelStream<R>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

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
    P: FnMut(usize),
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
            (this.on_progress)(num_samples)
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

pin_project! {
    /// Filters a [`AsyncReadSamples`] into a [`Stream`] by applying a [`Scanner`].
    ///
    /// Whenever the [`Scanner`] returns [`Some`], the stream yields an element.
    #[derive(Debug)]
    pub struct Filter<R, Sc> {
        #[pin]
        pub inner: R,
        pub scanner: Sc,
    }
}

impl<R, Sc> Filter<R, Sc> {
    pub fn new(inner: R, scanner: Sc) -> Self {
        Self { inner, scanner }
    }
}

impl<R, Sc, T> Stream for Filter<R, Sc>
where
    R: AsyncReadSamples,
    Sc: Scanner<R::Sample, Output = Option<T>>,
{
    type Item = Result<T, R::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // todo: use buffer?

        let mut this = self.project();

        let mut buffer = [MaybeUninit::uninit(); 1];

        loop {
            let mut read_buf = ReadBuf::uninit(UninitSlice::slice_mut_from_uninit(&mut buffer));
            match this.inner.as_mut().poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Some(Err(error))),
                Poll::Ready(Ok(())) => {
                    let n = read_buf.filled().len();

                    if n == 0 {
                        // end of stream
                        return Poll::Ready(None);
                    }
                    else {
                        assert!(n == 1);
                        let sample = unsafe {
                            // SAFETY: The buffer had one element written to it,
                            // so this must be initialized.
                            MaybeUninit::assume_init_read(&buffer[0])
                        };

                        if let Some(output) = this.scanner.scan(sample) {
                            return Poll::Ready(Some(Ok(output)));
                        }
                    }
                }
            }
        }
    }
}
