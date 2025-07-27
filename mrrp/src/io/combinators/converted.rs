use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use pin_project_lite::pin_project;

use crate::{
    io::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
        StreamLength,
        combinators::{
            ConvertScanner,
            scan::ScanWith,
        },
    },
    sample::FromSample,
};

pin_project! {
    #[derive(Clone, Debug)]
    pub struct Converted<R, S, Q> {
        #[pin]
        inner: ScanWith<R, S, ConvertScanner<Q>>,
    }
}

impl<R, S, Q> Converted<R, S, Q> {
    #[inline]
    pub fn new(inner: R) -> Self {
        Self {
            inner: ScanWith::new(inner, ConvertScanner::new()),
        }
    }
}

impl<R, S, Q> AsyncReadSamples<Q> for Converted<R, S, Q>
where
    R: AsyncReadSamples<S>,
    Q: FromSample<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Q>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_read_samples(cx, buffer)
    }
}

impl<R, S, Q> GetSampleRate for Converted<R, S, Q>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, S, Q> StreamLength for Converted<R, S, Q>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}
