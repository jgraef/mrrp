use std::{
    fmt::Debug,
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::{
    buf::SampleBufMut,
    sample::encoding::{
        Bytes,
        Decode,
        Encoding,
        Endianess,
    },
    signal::{
        AsyncReadSamples,
        ReadBuf,
        Remaining,
        StreamLength,
    },
};
use pin_project_lite::pin_project;
use tokio::io::AsyncRead;

pin_project! {
    /// Reads `u8` or `Complex<u8>` samples from a byte stream.
    pub struct PcmReader<R, S, E>
    where
        S: Decode<E>,
        E: Endianess,
    {
        #[pin]
        reader: R,
        buffer: <S as Encoding>::Encoded,
        buffer_filled: usize,
        _marker: PhantomData<fn() -> E>,
    }
}

impl<R, S, E> PcmReader<R, S, E>
where
    S: Decode<E>,
    E: Endianess,
{
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Bytes::zero(),
            buffer_filled: 0,
            _marker: PhantomData,
        }
    }
}

impl<R, S, E> Clone for PcmReader<R, S, E>
where
    R: Clone,
    S: Decode<E>,
    E: Endianess,
{
    fn clone(&self) -> Self {
        Self {
            reader: self.reader.clone(),
            buffer: self.buffer.clone(),
            buffer_filled: self.buffer_filled,
            _marker: PhantomData,
        }
    }
}

impl<R, S, E> Debug for PcmReader<R, S, E>
where
    R: Debug,
    S: Decode<E>,
    E: Endianess,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PcmReader")
            .field("reader", &self.reader)
            .field("buffer", &self.buffer)
            .field("buffer_filled", &self.buffer_filled)
            .finish()
    }
}

impl<R, S, E> AsyncReadSamples for PcmReader<R, S, E>
where
    R: AsyncRead,
    S: Decode<E>,
    E: Endianess,
{
    type Sample = S;
    type Error = std::io::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        // note: we're not sure how performant this is, since the loop does one
        // sample at a time. but otherwise we would need a larger internal
        // buffer. imho the buffering should be handled by the underlying
        // reader. but it would be wise to benchmark this.

        loop {
            let this = self.as_mut().project();

            // amount remaining in input buffer
            let remaining = buffer.remaining_mut();
            if remaining == 0 {
                // early return if input buffer is empty
                break;
            }

            // create tokio::io::ReadBuf with our small internal buffer - only
            // passing the portion that isn't filled yet.
            let mut read_buf =
                tokio::io::ReadBuf::new(&mut this.buffer.as_slice_mut()[*this.buffer_filled..]);

            match this.reader.poll_read(cx, &mut read_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    if read_buf.filled().len() == 0 {
                        // poll_read didn't put any more bytes into our buffer,
                        // meaning EOF
                        break;
                    }

                    // any bytes in the tokio ReadBuf are new (since we only
                    // passed it the unfilled portion of our buffer), so we add
                    // them to our counter.
                    *this.buffer_filled += read_buf.filled().len();

                    if *this.buffer_filled == <<S as Encoding>::Encoded>::LEN {
                        // small internal buffer contains enough to create one
                        // sample
                        let sample = S::decode(this.buffer);
                        buffer.put_sample(sample);
                        *this.buffer_filled = 0;
                    }
                }
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl<R, S, E> StreamLength for PcmReader<R, S, E>
where
    S: Decode<E>,
    E: Endianess,
{
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }
}
