mod combinators;
mod generator;
mod noise;
pub mod pcm;
mod read;
mod sine;
pub mod test;

use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_core::ready;
use mrrp_core::{
    buf::UninitSlice,
    signal::{
        AsyncReadSamples,
        AsyncWriteSamples,
        ReadBuf,
    },
};
use pin_project_lite::pin_project;

pub use crate::signal::{
    combinators::*,
    generator::*,
    noise::*,
    read::*,
    sine::*,
};

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

impl<S> Default for Buffer<S> {
    #[inline]
    fn default() -> Self {
        Self::new(0)
    }
}

impl<S> Buffer<S> {
    #[inline]
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer: UninitSlice::box_new(buffer_size),
            read_pos: 0,
            write_pos: 0,
        }
    }

    #[inline]
    pub fn grow(&mut self, new_size: usize) {
        if new_size > self.buffer.len() {
            self.resize(new_size);
        }
    }

    pub fn resize(&mut self, new_size: usize) {
        let mut buffer = UninitSlice::box_new(new_size);

        let mut read_pos = self.read_pos.min(new_size);
        let mut write_pos = self.write_pos.min(new_size);

        buffer[self.read_pos..self.write_pos].copy_from_uninit(&self.buffer[read_pos..write_pos]);

        unsafe {
            self.buffer[write_pos..self.write_pos].assume_init_drop();
        }

        if read_pos == write_pos {
            read_pos = 0;
            write_pos = 0;
        }

        self.read_pos = read_pos;
        self.write_pos = write_pos;
        self.buffer = buffer;
    }

    pub fn read(&mut self, buffer: &mut ReadBuf<S>) -> usize {
        let n = buffer.remaining().min(self.write_pos - self.read_pos);

        buffer.unfilled_mut()[..n].copy_from_uninit(&self.buffer[self.read_pos..][..n]);

        unsafe {
            buffer.assume_init(n);
        }
        buffer.set_filled(buffer.filled().len() + n);

        self.read_pos += n;
        if self.read_pos == self.write_pos {
            self.read_pos = 0;
            self.write_pos = 0;
        }

        n
    }

    pub fn poll_fill<R>(
        &mut self,
        cx: &mut Context<'_>,
        stream: Pin<&mut R>,
    ) -> Poll<Result<usize, R::Error>>
    where
        R: AsyncReadSamples<S>,
    {
        if self.read_pos == 0 && self.write_pos == 0 {
            let mut read_buf = ReadBuf::uninit(&mut self.buffer);

            ready!(stream.poll_read_samples(cx, &mut read_buf))?;
            self.write_pos = read_buf.filled().len();
            unsafe {
                read_buf.drop_unfilled_initialized();
            }

            Poll::Ready(Ok(self.write_pos))
        }
        else {
            debug_assert!(self.read_pos < self.write_pos);
            Poll::Ready(Ok(self.write_pos - self.read_pos))
        }
    }

    pub fn drain(&mut self, num_samples: usize) -> BufferDrain<'_, S> {
        let remaining = num_samples.min(self.write_pos - self.read_pos);
        BufferDrain {
            buffer: self,
            remaining,
        }
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.write_pos - self.read_pos
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

struct BufferDrain<'a, S> {
    buffer: &'a mut Buffer<S>,
    remaining: usize,
}

impl<'a, S> Drop for BufferDrain<'a, S> {
    fn drop(&mut self) {
        unsafe {
            self.buffer.buffer[self.buffer.read_pos..][..self.remaining].assume_init_drop();
        }
        self.buffer.read_pos += self.remaining;
        if self.buffer.read_pos == self.buffer.write_pos {
            self.buffer.read_pos = 0;
            self.buffer.write_pos = 0;
        }
    }
}

impl<'a, S> Iterator for BufferDrain<'a, S> {
    type Item = S;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining > 0 {
            debug_assert!(self.buffer.read_pos < self.buffer.write_pos);
            let sample = unsafe { self.buffer.buffer[self.buffer.read_pos].assume_init_read() };
            self.buffer.read_pos += 1;
            self.remaining -= 1;
            Some(sample)
        }
        else {
            None
        }
    }
}

#[derive(Debug)]
struct ScratchBuffer<S> {
    buffer: Box<UninitSlice<S>>,
}

impl<S> ScratchBuffer<S> {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer: UninitSlice::box_new(buffer_size),
        }
    }

    pub fn reserve(&mut self, length: usize) {
        if length > self.buffer.len() {
            *self = Self::new(length);
        }
    }
}

impl<S> Clone for ScratchBuffer<S> {
    fn clone(&self) -> Self {
        Self {
            buffer: UninitSlice::box_new(self.buffer.len()),
        }
    }
}
