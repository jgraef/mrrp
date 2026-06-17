use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::signal::{
    FiniteStream,
    GetSampleRate,
    Remaining,
    SizeHint,
    StreamLength,
};
use pin_project_lite::pin_project;

use crate::signal::{
    AsyncReadSamples,
    Buffer,
    ReadBuf,
};

pin_project! {
    #[derive(Clone, Debug)]
    pub struct Buffered<R>
    where
        R: AsyncReadSamples
    {
        #[pin]
        inner: R,
        buffer: Buffer<R::Sample>,
    }
}

impl<R> Buffered<R>
where
    R: AsyncReadSamples,
{
    #[inline]
    pub fn new(inner: R, buffer_size: usize) -> Self {
        Self {
            inner,
            buffer: Buffer::new(buffer_size),
        }
    }
}

impl<R> AsyncReadSamples for Buffered<R>
where
    R: AsyncReadSamples,
{
    type Sample = R::Sample;
    type Error = R::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<R::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let mut inner_is_pending = false;
        let mut have_filled_buf = false;

        while buffer.remaining() > 0 {
            let this = self.as_mut().project();

            if this.buffer.read_pos < this.buffer.write_pos {
                // we still have data buffered, so lets cosume that first.

                let num_samples = this.buffer.read(buffer);
                assert!(num_samples != 0);
                have_filled_buf = true;
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

                    match this.buffer.poll_fill(cx, this.inner) {
                        Poll::Pending => {
                            inner_is_pending = true;
                            break;
                        }
                        Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                        Poll::Ready(Ok(num_samples)) => {
                            if num_samples == 0 {
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

impl<R> GetSampleRate for Buffered<R>
where
    R: AsyncReadSamples + GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R> StreamLength for Buffered<R>
where
    R: AsyncReadSamples + StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining() + self.buffer.remaining()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint() + self.buffer.remaining()
    }
}

impl<R> FiniteStream for Buffered<R> where R: AsyncReadSamples + FiniteStream {}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use crate::signal::{
        AsyncReadSamplesExt,
        Cursor,
    };

    #[test]
    fn it_reads_straight_to_destination_buffer() {
        let samples = (0..100).collect::<Vec<_>>();
        let mut buffered = Cursor::new(&samples[..]).buffered(50);
        let mut destination = vec![0; 70];

        // since the destination buffer is larger than the internal buffer this should
        // read straight to the destination buffer
        buffered
            .read_samples(&mut destination[..])
            .now_or_never()
            .expect("pending")
            .unwrap();

        assert_eq!(buffered.inner.position(), 70);
        assert_eq!(buffered.buffer.read_pos, 0);
        assert_eq!(buffered.buffer.write_pos, 0);
        destination
            .iter()
            .enumerate()
            .for_each(|(i, sample)| assert_eq!(samples[i], *sample));
    }

    #[test]
    fn it_buffers_if_necessary() {
        let samples = (0..100).collect::<Vec<_>>();
        let mut buffered = Cursor::new(&samples[..]).buffered(50);
        let mut destination = vec![0; 20];

        // since the destination buffer is smaller than the internal buffer this should
        // read first into the internal buffer
        buffered
            .read_samples(&mut destination[..])
            .now_or_never()
            .expect("pending")
            .unwrap();

        assert_eq!(buffered.inner.position(), 50);
        assert_eq!(buffered.buffer.read_pos, 20);
        assert_eq!(buffered.buffer.write_pos, 50);
        destination
            .iter()
            .enumerate()
            .for_each(|(i, sample)| assert_eq!(samples[i], *sample));
    }

    #[test]
    fn successive_reads_will_be_served_by_the_buffer() {
        let samples = (0..100).collect::<Vec<_>>();
        let mut buffered = Cursor::new(&samples[..]).buffered(50);
        let mut destination = vec![0; 10];

        for i in 0..4 {
            buffered
                .read_samples(&mut destination[..])
                .now_or_never()
                .expect("pending")
                .unwrap();
            assert_eq!(buffered.inner.position(), 50);
            assert_eq!(buffered.buffer.read_pos, (i + 1) * 10);
            assert_eq!(buffered.buffer.write_pos, 50);
            destination
                .iter()
                .enumerate()
                .for_each(|(j, sample)| assert_eq!(samples[i * 10 + j], *sample));
        }

        // the last read will drain the buffer, resetting the pointers
        buffered
            .read_samples(&mut destination[..])
            .now_or_never()
            .expect("pending")
            .unwrap();
        assert_eq!(buffered.buffer.read_pos, 0);
        assert_eq!(buffered.buffer.write_pos, 0);
        destination
            .iter()
            .enumerate()
            .for_each(|(i, sample)| assert_eq!(samples[40 + i], *sample));

        // the next read will fill the buffer again
        buffered
            .read_samples(&mut destination[..])
            .now_or_never()
            .expect("pending")
            .unwrap();
        assert_eq!(buffered.inner.position(), 100);
        assert_eq!(buffered.buffer.read_pos, 10);
        assert_eq!(buffered.buffer.write_pos, 50);
        destination
            .iter()
            .enumerate()
            .for_each(|(i, sample)| assert_eq!(samples[50 + i], *sample));
    }

    #[test]
    fn it_reads_if_the_buffer_has_only_partial_data() {
        let samples = (0..100).collect::<Vec<_>>();
        let mut buffered = Cursor::new(&samples[..]).buffered(50);
        let mut destination = vec![0; 50];

        // first do a small read to fill the buffer
        buffered
            .read_samples(&mut destination[..10])
            .now_or_never()
            .expect("pending")
            .unwrap();

        assert_eq!(buffered.inner.position(), 50);
        assert_eq!(buffered.buffer.read_pos, 10);
        assert_eq!(buffered.buffer.write_pos, 50);
        destination[..10]
            .iter()
            .enumerate()
            .for_each(|(i, sample)| assert_eq!(samples[i], *sample));

        // now do a larger read that can't be completely filled by the buffer.
        let num_samples_read = buffered
            .read_samples(&mut destination[..])
            .now_or_never()
            .expect("pending")
            .unwrap();

        assert_eq!(num_samples_read, 50);
        assert_eq!(buffered.inner.position(), 100);
        assert_eq!(buffered.buffer.read_pos, 10);
        assert_eq!(buffered.buffer.write_pos, 50);
        destination
            .iter()
            .enumerate()
            .for_each(|(i, sample)| assert_eq!(samples[10 + i], *sample));
    }
}
