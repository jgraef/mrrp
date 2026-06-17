use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::{
    sample::FromSample,
    signal::{
        FiniteStream,
        GetSampleRate,
        Remaining,
        StreamLength,
    },
};
use pin_project_lite::pin_project;

use crate::signal::{
    AsyncReadSamples,
    ReadBuf,
    combinators::{
        ConvertScanner,
        scan::ScanWith,
    },
};

pin_project! {
    #[derive(derive_more::Debug)]
    #[debug(bound(R: AsyncReadSamples + Debug, R::Sample: Debug))]
    pub struct Converted<R, Q>
    where
        R: AsyncReadSamples,
    {
        #[pin]
        inner: ScanWith<R, ConvertScanner<Q>>,
    }
}

impl<R, Q> Clone for Converted<R, Q>
where
    R: AsyncReadSamples + Clone,
    R::Sample: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<R, Q> Converted<R, Q>
where
    R: AsyncReadSamples,
{
    #[inline]
    pub fn new(inner: R) -> Self {
        Self {
            inner: ScanWith::new(inner, ConvertScanner::new()),
        }
    }

    pub fn inner(&self) -> &R {
        self.inner.inner()
    }

    pub fn inner_mut(&mut self) -> &mut R {
        self.inner.inner_mut()
    }
}

impl<R, Q> AsyncReadSamples for Converted<R, Q>
where
    R: AsyncReadSamples,
    Q: FromSample<R::Sample>,
{
    type Sample = Q;
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Q>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_read_samples(cx, buffer)
    }
}

impl<R, Q> GetSampleRate for Converted<R, Q>
where
    R: AsyncReadSamples + GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, Q> StreamLength for Converted<R, Q>
where
    R: AsyncReadSamples + StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R, Q> FiniteStream for Converted<R, Q> where R: AsyncReadSamples + FiniteStream {}
