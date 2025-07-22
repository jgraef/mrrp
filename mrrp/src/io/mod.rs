mod read;
mod write;

use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use pin_project_lite::pin_project;
use tracing::Span;

pub use self::{
    read::*,
    write::*,
};
use crate::buf::UninitSlice;

pin_project! {
    #[derive(Debug)]
    #[must_use]
    pub struct Forward<R, W, S> {
        #[pin]
        source: R,
        #[pin]
        sink: W,
        buffer: Buffer<S>,
        num_samples_written: usize,
    }
}

impl<R, W, S> Forward<R, W, S> {
    pub fn new(source: R, sink: W, buffer_size: usize) -> Self {
        Self {
            source,
            sink,
            buffer: Buffer::new(buffer_size),
            num_samples_written: 0,
        }
    }
}

impl<R, W, S> Future for Forward<R, W, S>
where
    R: AsyncReadSamples<S>,
    W: AsyncWriteSamples<S>,
{
    type Output = Result<usize, ForwardError<R::Error, W::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let this = self.as_mut().project();

            if this.buffer.read_pos < this.buffer.write_pos {
                // we still have data buffered, so lets cosume that first.

                let buffer = unsafe {
                    this.buffer.buffer[this.buffer.read_pos..this.buffer.write_pos]
                        .assume_init_ref()
                };

                match this.sink.poll_write_samples(cx, buffer) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(ForwardError::Sink(error))),
                    Poll::Ready(Ok(num_samples_consumed)) => {
                        assert!(num_samples_consumed <= buffer.len());

                        this.buffer.read_pos += num_samples_consumed;
                        *this.num_samples_written += num_samples_consumed;

                        if this.buffer.read_pos == this.buffer.write_pos {
                            this.buffer.read_pos = 0;
                            this.buffer.write_pos = 0;
                        }
                    }
                }
            }
            else {
                // we need to read new data

                assert!(this.buffer.read_pos == 0);
                assert!(this.buffer.write_pos == 0);

                let mut read_buf = ReadBuf::uninit(&mut this.buffer.buffer);
                match this.source.poll_read_samples(cx, &mut read_buf) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => {
                        return Poll::Ready(Err(ForwardError::Source(error)));
                    }
                    Poll::Ready(Ok(())) => {
                        this.buffer.write_pos = read_buf.filled().len();
                        unsafe {
                            read_buf.drop_unfilled_initialized();
                        }

                        if this.buffer.write_pos == 0 {
                            // if the read returned nothing, this is EOF
                            break;
                        }
                    }
                }
            }
        }

        Poll::Ready(Ok(self.num_samples_written))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("forward error")]
pub enum ForwardError<R, W> {
    Source(#[source] R),
    Sink(#[source] W),
}

/// The buffer used for [`Buffered`] and [`Forward`]. Ideally this would just be
/// a SamplesMut, or at least have a proper API
#[derive(Debug)]
struct Buffer<S> {
    buffer: Box<UninitSlice<S>>,
    read_pos: usize,
    write_pos: usize,
}

impl<S> Buffer<S> {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer: UninitSlice::box_new(buffer_size),
            read_pos: 0,
            write_pos: 0,
        }
    }
}

impl<S> Clone for Buffer<S>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        let mut buffer = UninitSlice::box_new(self.buffer.len());

        let filled = unsafe { self.buffer[self.read_pos..self.write_pos].assume_init_ref() };
        buffer[self.read_pos..self.write_pos].clone_from_slice(filled);

        Self {
            buffer,
            read_pos: self.read_pos,
            write_pos: self.write_pos,
        }
    }
}

impl<S> Drop for Buffer<S> {
    fn drop(&mut self) {
        // everything in read_pos..write_pos is initialized, so we need to drop it
        unsafe {
            self.buffer[self.read_pos..self.write_pos].assume_init_drop();
        }
    }
}

pin_project! {
    #[derive(Clone, Debug)]
    pub struct WithSpan<T> {
        #[pin]
        inner: T,
        span: Span,
    }
}

impl<T, S> AsyncReadSamples<S> for WithSpan<T>
where
    T: AsyncReadSamples<S>,
{
    type Error = T::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        let _guard = this.span.enter();
        this.inner.poll_read_samples(cx, buffer)
    }
}

impl<T, S> AsyncWriteSamples<S> for WithSpan<T>
where
    T: AsyncWriteSamples<S>,
{
    type Error = T::Error;

    #[inline]
    fn poll_write_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        let this = self.project();
        let _guard = this.span.enter();
        this.inner.poll_write_samples(cx, buffer)
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        let _guard = this.span.enter();
        this.inner.poll_flush(cx)
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        let _guard = this.span.enter();
        this.inner.poll_close(cx)
    }
}
