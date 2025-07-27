use std::{
    hint::black_box,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use pin_project_lite::pin_project;

use crate::{
    buf::SampleBufMut,
    io::{
        AsyncReadSamples,
        FiniteStream,
        ReadBuf,
        Remaining,
        StreamLength,
    },
};

pin_project! {
    /// Stream that only ever returns a single sample
    #[derive(Clone, Copy, Debug)]
    pub struct SingleSampleStream<R> {
        #[pin]
        inner: R,
    }
}

impl<R> SingleSampleStream<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R, S> AsyncReadSamples<S> for SingleSampleStream<R>
where
    R: AsyncReadSamples<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        if buffer.has_remaining_mut() {
            let mut read_buf = buffer.take(1);
            match self.project().inner.poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let filled = read_buf.filled().len();
                    let initialized = read_buf.initialized().len();

                    assert!(filled <= 1);
                    if filled == 1 {
                        unsafe {
                            buffer.assume_init(initialized);
                        }
                        buffer.set_filled(buffer.filled().len() + filled);
                    }
                }
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl<R> StreamLength for SingleSampleStream<R>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R> FiniteStream for SingleSampleStream<R> where R: FiniteStream {}

pin_project! {
    /// Stream that passes the buffer and result through [`black_box`].
    #[derive(Clone, Copy, Debug)]
    pub struct BlackBoxStream<R> {
        #[pin]
        inner: R,
    }
}

impl<R> BlackBoxStream<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R, S> AsyncReadSamples<S> for BlackBoxStream<R>
where
    R: AsyncReadSamples<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        black_box(
            self.project()
                .inner
                .poll_read_samples(cx, black_box(buffer)),
        )
    }
}

impl<R> StreamLength for BlackBoxStream<R>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R> FiniteStream for BlackBoxStream<R> where R: FiniteStream {}
