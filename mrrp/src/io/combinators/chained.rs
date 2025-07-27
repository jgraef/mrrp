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
        GetSampleRate,
        ReadBuf,
        StreamLength,
    },
};

pin_project! {
    #[derive(Debug)]
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

impl<H, T, S> AsyncReadSamples<S> for Chained<H, T>
where
    H: AsyncReadSamples<S>,
    T: AsyncReadSamples<S>,
{
    type Error = ChainedError<H::Error, T::Error>;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
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
    fn remaining(&self) -> usize {
        let head = if self.head_exhausted {
            0
        }
        else {
            self.head.remaining()
        };
        head + self.tail.remaining()
    }
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("chained stream error")]
pub enum ChainedError<H, T> {
    Head(H),
    Tail(T),
}
