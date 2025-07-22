use std::{
    convert::Infallible,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytes::BufMut;
use num_complex::Complex;
use pin_project_lite::pin_project;
use tokio::io::AsyncWrite;

use crate::io::AsyncWriteSamples;

#[derive(Debug, thiserror::Error)]
#[error("raw sample writer error")]
pub enum RawWriterError<E> {
    Writer(#[source] std::io::Error),
    Encode(#[source] E),
}

pin_project! {
    #[derive(Clone, Debug)]
    pub struct RawAsyncWriter<W> {
        #[pin]
        writer: W,
        write_pos: usize,
        buffer: Vec<u8>,
    }
}

impl<W> RawAsyncWriter<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            write_pos: 0,
            buffer: vec![],
        }
    }
}

impl<W, S> AsyncWriteSamples<S> for RawAsyncWriter<W>
where
    W: AsyncWrite,
    S: EncodeSample,
{
    type Error = RawWriterError<<S as EncodeSample>::Error>;

    fn poll_write_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        let mut num_samples_consumed = 0;
        let mut inner_pending = false;

        loop {
            let this = self.as_mut().project();

            if this.buffer.is_empty() && !buffer.is_empty() {
                assert_eq!(*this.write_pos, 0);

                this.buffer.resize(S::BYTES * buffer.len(), 0);

                let mut write_pos = 0;
                for sample in buffer {
                    sample
                        .encode(&mut this.buffer[write_pos..][..S::BYTES])
                        .map_err(RawWriterError::Encode)?;
                    write_pos += S::BYTES;
                }
                assert_eq!(write_pos, this.buffer.len());

                num_samples_consumed += buffer.len();
                buffer = &[];
            }
            else {
                match this.writer.poll_write(cx, &this.buffer[*this.write_pos..]) {
                    Poll::Pending => {
                        inner_pending = true;
                        break;
                    }
                    Poll::Ready(Err(error)) => {
                        return Poll::Ready(Err(RawWriterError::Writer(error)));
                    }
                    Poll::Ready(Ok(num_bytes_written)) => {
                        if num_bytes_written == 0 {
                            break;
                        }

                        *this.write_pos += num_bytes_written;
                        if *this.write_pos == this.buffer.len() {
                            *this.write_pos = 0;
                            this.buffer.clear();
                        }
                        else {
                            assert!(*this.write_pos < this.buffer.len());
                        }
                    }
                }
            }
        }

        if num_samples_consumed == 0 && inner_pending {
            Poll::Pending
        }
        else {
            Poll::Ready(Ok(num_samples_consumed))
        }
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project()
            .writer
            .poll_flush(cx)
            .map_err(RawWriterError::Writer)
    }

    #[inline]
    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.project()
            .writer
            .poll_shutdown(cx)
            .map_err(RawWriterError::Writer)
    }
}

pub trait EncodeSample {
    type Error;

    const BYTES: usize;

    fn encode(&self, buffer: &mut [u8]) -> Result<(), Self::Error>;
}

macro_rules! impl_encode_sample {
    {$($T:ty, $bytes:expr, $method:ident;)*} => {
        $(
            impl EncodeSample for $T {
                type Error = Infallible;

                const BYTES: usize = $bytes;

                fn encode(&self, mut buffer: &mut [u8]) -> Result<(), Self::Error> {
                    buffer.$method(*self);
                    Ok(())
                }
            }

            impl EncodeSample for Complex<$T> {
                type Error = Infallible;

                const BYTES: usize = $bytes * 2;

                fn encode(&self, mut buffer: &mut [u8]) -> Result<(), Self::Error> {
                    buffer.$method(self.re);
                    buffer.$method(self.im);
                    Ok(())
                }
            }
        )*
    };
}

impl_encode_sample! {
    u8, 1, put_u8;
    i8, 1, put_i8;
    u16, 2, put_u16;
    i16, 2, put_i16;
    u32, 4, put_u32;
    i32, 4, put_i32;
    u64, 8, put_u64;
    i64, 8, put_i64;
    f32, 4, put_f32;
    f64, 8, put_f64;
}
