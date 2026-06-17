use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::{
    buf::SampleBufMut,
    signal::{
        FiniteStream,
        GetSampleRate,
        Remaining,
        SizeHint,
        StreamLength,
    },
};
use pin_project_lite::pin_project;

use crate::signal::{
    AsyncReadSamples,
    ReadBuf,
};

pin_project! {
    #[derive(Clone, Copy, Debug)]
    pub struct Chained<H, T> {
        #[pin]
        head: H,
        head_exhausted: bool,
        #[pin]
        tail: T,
    }
}

impl<H, T> Chained<H, T> {
    #[inline]
    pub fn new(head: H, tail: T) -> Self {
        Self {
            head,
            head_exhausted: false,
            tail,
        }
    }
}

impl<H, T> AsyncReadSamples for Chained<H, T>
where
    H: AsyncReadSamples,
    T: AsyncReadSamples<Sample = H::Sample>,
{
    type Sample = H::Sample;
    type Error = ChainedError<H::Error, T::Error>;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        if !buffer.has_remaining_mut() {
            return Poll::Ready(Ok(()));
        }

        let filled_before = buffer.filled().len();

        loop {
            let this = self.as_mut().project();

            if *this.head_exhausted {
                match this.tail.poll_read_samples(cx, buffer) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(ChainedError::Tail(error))),
                    Poll::Ready(Ok(())) => return Poll::Ready(Ok(())),
                }
            }
            else {
                match this.head.poll_read_samples(cx, buffer) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(ChainedError::Head(error))),
                    Poll::Ready(Ok(())) => {
                        if buffer.filled().len() == filled_before {
                            // head is exhausted
                            *this.head_exhausted = true;
                        }
                        else {
                            return Poll::Ready(Ok(()));
                        }
                    }
                }
            }
        }
    }
}

impl<H, T> GetSampleRate for Chained<H, T>
where
    H: GetSampleRate,
    T: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        if self.head_exhausted {
            self.tail.sample_rate()
        }
        else {
            self.tail.sample_rate()
        }
    }
}

impl<H, T> StreamLength for Chained<H, T>
where
    H: StreamLength,
    T: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        if self.head_exhausted {
            self.tail.remaining()
        }
        else {
            self.head.remaining() + self.tail.remaining()
        }
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        if self.head_exhausted {
            self.tail.size_hint()
        }
        else {
            self.head.size_hint() + self.tail.size_hint()
        }
    }
}

impl<H, T> FiniteStream for Chained<H, T>
where
    H: FiniteStream,
    T: FiniteStream,
{
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("chained stream error")]
pub enum ChainedError<H, T> {
    Head(H),
    Tail(T),
}
