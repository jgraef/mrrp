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
    sample::encoding::{
        Bytes,
        Encode,
        Encoding,
        Endianess,
    },
    signal::AsyncWriteSamples,
};
use pin_project_lite::pin_project;
use tokio::io::AsyncWrite;

pin_project! {
    /// Sink that writes samples to a [`AsyncWrite`].
    pub struct PcmWriter<W, S, E>
    where
        S: Encoding,
        E: Endianess,
    {
        #[pin]
        writer: W,
        buffer: <S as Encoding>::Encoded,
        buffer_position: usize,
        _marker: PhantomData<fn() -> E>,
    }
}

impl<W, S, E> PcmWriter<W, S, E>
where
    S: Encoding,
    E: Endianess,
{
    #[inline]
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            buffer: Bytes::zero(),
            buffer_position: <<S as Encoding>::Encoded>::LEN,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn into_inner(self) -> W {
        self.writer
    }

    #[inline]
    pub fn get_ref(&self) -> &W {
        &self.writer
    }
}

impl<W, S, E> Clone for PcmWriter<W, S, E>
where
    W: Clone,
    S: Encode<E>,
    E: Endianess,
{
    fn clone(&self) -> Self {
        Self {
            writer: self.writer.clone(),
            buffer: self.buffer.clone(),
            buffer_position: self.buffer_position,
            _marker: PhantomData,
        }
    }
}

impl<W, S, E> Debug for PcmWriter<W, S, E>
where
    W: Debug,
    S: Encode<E>,
    E: Endianess,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PcmWriter")
            .field("writer", &self.writer)
            .field("buffer", &self.buffer)
            .field("buffer_position", &self.buffer_position)
            .finish()
    }
}

impl<W, S, E> AsyncWriteSamples<S> for PcmWriter<W, S, E>
where
    W: AsyncWrite,
    S: Encode<E>,
    E: Endianess,
{
    type Error = std::io::Error;

    fn poll_write_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut num_samples_written = 0;

        loop {
            let this = self.as_mut().project();

            if *this.buffer_position == <<S as Encoding>::Encoded>::LEN {
                // our small internal buffer is empty, so we can attempt to
                // write a full sample

                // if the input buffer is empty, we're done.
                if buffer.is_empty() {
                    break;
                }

                // get one sample and increment cursor.
                *this.buffer = buffer[0].encode();
                buffer = &buffer[1..];
                num_samples_written += 1;

                // set small internal buffer filled, so the else case will write
                // it
                *this.buffer_position = 0;
            }
            else {
                // there's some left-over in the internal buffer that we weren't
                // able to write previously. so we need to write it first.

                match this
                    .writer
                    .poll_write(cx, &this.buffer.as_slice()[*this.buffer_position..])
                {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Ready(Ok(num_bytes_written)) => {
                        if num_bytes_written == 0 {
                            // underlying writer doesn't accept any bytes anmore
                            break;
                        }
                        *this.buffer_position += num_bytes_written;
                    }
                }
            }
        }

        Poll::Ready(Ok(num_samples_written))
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().writer.poll_flush(cx)
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project().writer.poll_shutdown(cx)
    }
}
