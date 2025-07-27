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
    GetSampleRate,
    ReadBuf,
    StreamLength,
};

pin_project! {
    #[derive(Debug)]
    pub struct Limited<R> {
        #[pin]
        inner: R,
        remaining: usize,
    }
}

impl<R> Limited<R> {
    pub fn new(inner: R, num_samples: usize) -> Self {
        Self {
            inner,
            remaining: num_samples,
        }
    }
}

impl<R, S> AsyncReadSamples<S> for Limited<R>
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
        if *this.remaining == 0 {
            Poll::Ready(Ok(()))
        }
        else {
            let mut read_buf = buffer.take(*this.remaining);
            match this.inner.poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let initialized = read_buf.initialized().len();
                    let filled = read_buf.filled().len();
                    unsafe {
                        buffer.assume_init(initialized);
                    }
                    buffer.set_filled(buffer.filled().len() + filled);
                    *this.remaining -= filled;
                    Poll::Ready(Ok(()))
                }
            }
        }
    }
}

impl<R> GetSampleRate for Limited<R>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R> StreamLength for Limited<R>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> usize {
        let inner = self.inner.remaining();
        inner.min(self.remaining)
    }
}
