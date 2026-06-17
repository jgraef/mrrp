//! # Warning
//!
//! **Completely untested!**
//!
//! # TODO
//!
//! - Write some tests
//! - Make a variant that takes advantage of `AsyncBufRead`

use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::{
    sample::Complex,
    signal::{
        AsyncReadSamples,
        ReadBuf,
        Remaining,
        StreamLength,
    },
};
use pin_project_lite::pin_project;

pin_project! {
    /// Reads `u8` or `Complex<u8>` samples from a byte stream.
    ///
    /// # Warning
    ///
    /// **Completely untested!**
    ///
    #[derive(Clone, Debug)]
    pub struct PcmSource<R, S> where S: DecodeSample {
        #[pin]
        reader: R,
        remainder_buffer: S::RemainderBuffer,
        _marker: PhantomData<fn() -> S>,
    }
}

impl<R, S> AsyncReadSamples for PcmSource<R, S>
where
    R: tokio::io::AsyncRead,
    S: DecodeSample,
{
    type Sample = S;
    type Error = std::io::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            let this = self.as_mut().project();

            let unfilled = buffer.unfilled_mut();
            if unfilled.is_empty() {
                return Poll::Ready(Ok(()));
            }

            let (head, body, tail) = unfilled.align_to_mut::<u8>();
            // there should be no mis-alignment, since we cast to [u8]
            assert!(head.is_empty());
            assert!(tail.is_empty());
            debug_assert_eq!(body.len(), body.len() * size_of::<S>());

            // how many bytes are still in our remainder buffer from before
            let remainder = this.remainder_buffer.get();

            // we're reading at least 1 sample, so the buffer must be larger than any
            // remainder
            assert!(remainder.len() < body.len());

            // skip the first couple of bytes for the remainder.
            // we copy the remainder into the buffer later, because `poll_read` might return
            // `Pending`
            let mut read_buf_bytes =
                tokio::io::ReadBuf::uninit(&mut body.as_mut_slice()[remainder.len()..]);

            match this.reader.poll_read(cx, &mut read_buf_bytes) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {}
            }

            if read_buf_bytes.filled().is_empty() {
                // got Ok(()), but no bytes filled. this means EOF
                return Poll::Ready(Ok(()));
            }

            // how many bytes are in the buffer now (including remainder)
            let n_bytes_filled = remainder.len() + read_buf_bytes.filled().len();
            // how many full samples we got
            let n_samples = n_bytes_filled / size_of::<S>();
            // new remainder length
            let n_remainder = n_bytes_filled % size_of::<S>();

            // todo: benchmark if that if is worth it, or use hints (requires unstable
            // feature)
            if !remainder.is_empty() {
                // fill in the first couple of bytes from remainder buffer
                body[..remainder.len()].clone_from_slice(remainder);
            }

            // and copy any new remainder to the remainder buffer
            let remainder = unsafe {
                // SAFETY: this was initialized by the call to poll_read
                body[n_samples * size_of::<S>()..].assume_init_ref()
            };
            debug_assert_eq!(remainder.len(), n_remainder);
            this.remainder_buffer.put(remainder);

            buffer.set_filled(buffer.filled().len() + n_samples);

            // we need special handling in case the underlying byte stream returns less than
            // a full sample. in that case we can't return now because we only
            // filled our remainder buffer, but not any samples into the buffer provided by
            // the user. this would make the user assume we're at EOF, but we
            // aren't. we can't return Poll::Pending, because the underlying byte stream
            // didn't.
            if n_samples > 0 {
                return Poll::Ready(Ok(()));
            }
        }
    }
}

impl<R, S> StreamLength for PcmSource<R, S>
where
    S: DecodeSample,
{
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }
}

pub unsafe trait DecodeSample {
    type RemainderBuffer: RemainderBuffer;
}

unsafe impl DecodeSample for u8 {
    type RemainderBuffer = ();
}

unsafe impl DecodeSample for i8 {
    type RemainderBuffer = Option<u8>;
}

unsafe impl DecodeSample for Complex<u8> {
    type RemainderBuffer = Option<u8>;
}

unsafe impl DecodeSample for Complex<i8> {
    type RemainderBuffer = Option<u8>;
}

impl RemainderBuffer for () {
    fn put(&mut self, bytes: &[u8]) {
        assert!(bytes.is_empty());
    }

    fn get(&mut self) -> &[u8] {
        &[]
    }
}

impl RemainderBuffer for Option<u8> {
    fn put(&mut self, bytes: &[u8]) {
        assert!(bytes.len() <= 1);
        if bytes.is_empty() {
            *self = None;
        }
        else {
            *self = Some(bytes[0]);
        }
    }

    fn get(&mut self) -> &[u8] {
        self.as_ref().map_or(&[], |byte| std::slice::from_ref(byte))
    }
}

pub trait RemainderBuffer {
    fn put(&mut self, bytes: &[u8]);
    fn get(&mut self) -> &[u8];
}
