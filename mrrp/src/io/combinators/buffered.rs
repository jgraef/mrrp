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
    Buffer,
    FiniteStream,
    GetSampleRate,
    ReadBuf,
    Remaining,
    StreamLength,
};

pin_project! {
    #[derive(Clone, Debug)]
    pub struct Buffered<R, S> {
        #[pin]
        inner: R,
        buffer: Buffer<S>,
    }
}

impl<R, S> Buffered<R, S> {
    pub fn new(inner: R, buffer_size: usize) -> Self {
        Self {
            inner,
            buffer: Buffer::new(buffer_size),
        }
    }
}

impl<R, S> AsyncReadSamples<S> for Buffered<R, S>
where
    R: AsyncReadSamples<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let mut inner_is_pending = false;
        let mut have_filled_buf = false;

        while buffer.remaining() > 0 {
            let mut this = self.as_mut().project();

            if this.buffer.read_pos < this.buffer.write_pos {
                // we still have data buffered, so lets cosume that first.

                let n = buffer
                    .remaining()
                    .min(this.buffer.write_pos - this.buffer.read_pos);

                buffer
                    .unfilled_mut()
                    .copy_from_uninit(&this.buffer.buffer[this.buffer.read_pos..][..n]);

                unsafe {
                    buffer.assume_init(n);
                }
                buffer.set_filled(buffer.filled().len() + n);
                have_filled_buf = true;

                this.buffer.read_pos += n;
                if this.buffer.read_pos == this.buffer.write_pos {
                    this.buffer.read_pos = 0;
                    this.buffer.write_pos = 0;
                }
            }
            else {
                if inner_is_pending {
                    break;
                }

                if this.buffer.buffer.len() > buffer.remaining() {
                    // our buffer size is larger than the remaining space in the destination buffer,
                    // so we'll read to our buffer.
                    assert_eq!(this.buffer.read_pos, 0);
                    assert_eq!(this.buffer.write_pos, 0);

                    let mut read_buf = ReadBuf::uninit(&mut this.buffer.buffer);

                    match this.inner.as_mut().poll_read_samples(cx, &mut read_buf) {
                        Poll::Pending => {
                            inner_is_pending = true;
                            // note that we can't break out of the outer loop, since we
                            // might have read something into our buffer in the last
                            // interation of this inner loop.
                            break;
                        }
                        Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                        Poll::Ready(Ok(())) => {
                            this.buffer.write_pos = read_buf.filled().len();
                            unsafe {
                                read_buf.drop_unfilled_initialized();
                            }

                            if this.buffer.write_pos == 0 {
                                // the read_buf wasn't filled with any bytes, so this is an eof.
                                break;
                            }
                        }
                    }
                }
                else {
                    // the destination buffer is larger than our buffer. instead of first reading to
                    // our buffer and then copying to the destination buffer, we can read directly
                    // to the destination buffer

                    let filled_before = buffer.filled().len();
                    match this.inner.poll_read_samples(cx, buffer) {
                        Poll::Pending => {
                            inner_is_pending = true;
                            break;
                        }
                        Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                        Poll::Ready(Ok(())) => {
                            if buffer.filled().len() == filled_before {
                                // eof
                                break;
                            }

                            // directly read to destination buffer, so nothing
                            // to do here.
                            have_filled_buf = true;
                        }
                    }
                }
            }
        }

        if inner_is_pending && !have_filled_buf {
            Poll::Pending
        }
        else {
            Poll::Ready(Ok(()))
        }
    }
}

impl<R, S> GetSampleRate for Buffered<R, S>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, S> StreamLength for Buffered<R, S>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R, S> FiniteStream for Buffered<R, S> where R: FiniteStream {}
