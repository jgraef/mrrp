use std::{
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
        AsyncReadSamplesExt,
        FiniteStream,
        ReadBuf,
        Remaining,
        StreamLength,
    },
};

pin_project! {
    /// Repeats another stream indefinitely.
    ///
    /// Be warned that this will try to buffer all samples from the input stream in
    /// order to repeat them. If the input stream is not limited in length this will
    /// exhaust your memory.
    #[derive(Clone, Debug)]
    pub struct Repeated<R, S> {
        #[pin]
        inner: R,
        inner_exhausted: bool,
        buffer: Vec<S>,
        read_pos: usize,
    }
}

impl<R, S> Repeated<R, S> {
    #[inline]
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            inner_exhausted: false,
            buffer: vec![],
            read_pos: 0,
        }
    }

    #[inline]
    pub async fn prefetch(&mut self) -> Result<(), R::Error>
    where
        R: AsyncReadSamples<S> + FiniteStream + Unpin,
    {
        self.inner.read_to_end(&mut self.buffer).await
    }

    #[inline]
    pub async fn prefetched(mut self) -> Result<Self, R::Error>
    where
        R: AsyncReadSamples<S> + FiniteStream + Unpin,
    {
        self.prefetch().await?;
        Ok(self)
    }
}

impl<R, S> AsyncReadSamples<S> for Repeated<R, S>
where
    S: Clone,
    R: AsyncReadSamples<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            let this = self.as_mut().project();

            if *this.inner_exhausted {
                let n = buffer.remaining_mut();
                if n == 0 {
                    return Poll::Ready(Ok(()));
                }

                let n = n.min(this.buffer.len() - *this.read_pos);

                buffer.put_slice(&this.buffer[*this.read_pos..][..n]);

                *this.read_pos += n;
                if *this.read_pos == this.buffer.len() {
                    *this.read_pos = 0;
                }
            }
            else {
                if this.buffer.is_empty() {
                    this.buffer
                        .reserve(this.inner.size_hint().buffer_size(buffer.remaining_mut()));
                }

                let filled_before = buffer.filled().len();
                match this.inner.poll_read_samples(cx, buffer) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Ready(Ok(())) => {
                        if buffer.filled().len() == filled_before {
                            *this.inner_exhausted = true;
                        }
                        else {
                            this.buffer
                                .extend(buffer.filled()[filled_before..].iter().cloned());
                            return Poll::Ready(Ok(()));
                        }
                    }
                }
            }
        }
    }
}

impl<R, S> StreamLength for Repeated<R, S> {
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Infinite
    }
}
