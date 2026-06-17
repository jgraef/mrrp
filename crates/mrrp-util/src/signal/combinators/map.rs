use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytemuck::{
    Pod,
    Zeroable,
};
use mrrp_core::signal::{
    FiniteStream,
    GetSampleRate,
    Remaining,
    StreamLength,
};
use pin_project_lite::pin_project;

use crate::signal::{
    AsyncReadSamples,
    ReadBuf,
    combinators::scan::{
        FuncScanner,
        ScanInPlaceWith,
        ScanWith,
    },
};

pin_project! {
    /// Stream wrapper that maps the samples using an intermediate buffer.
    #[derive(derive_more::Debug)]
    #[debug(bound(R: AsyncReadSamples, R::Sample: Debug))]
    pub struct Map<R, F>
    where R: AsyncReadSamples
    {
        #[pin]
        inner: ScanWith<R, FuncScanner<F>>,
    }
}

impl<R, F> Clone for Map<R, F>
where
    R: AsyncReadSamples + Clone,
    F: Clone,
    R::Sample: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<R, F> Map<R, F>
where
    R: AsyncReadSamples,
{
    #[inline]
    pub fn new(inner: R, f: F) -> Self {
        Self {
            inner: ScanWith::new(inner, FuncScanner::new(f)),
        }
    }

    #[inline]
    pub fn with_max_buffer_size(self, max_buffer_size: usize) -> Self {
        Self {
            inner: self.inner.with_max_buffer_size(max_buffer_size),
        }
    }
}

impl<R, Q, F> AsyncReadSamples for Map<R, F>
where
    R: AsyncReadSamples,
    F: FnMut(R::Sample) -> Q,
{
    type Sample = Q;
    type Error = R::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Q>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_read_samples(cx, buffer)
    }
}

impl<R, F> GetSampleRate for Map<R, F>
where
    R: AsyncReadSamples + GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, F> StreamLength for Map<R, F>
where
    R: AsyncReadSamples + StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R, F> FiniteStream for Map<R, F> where R: AsyncReadSamples + FiniteStream {}

pin_project! {
    /// Stream wrapper that maps the samples using an intermediate buffer.
    #[derive(Clone, Copy, Debug)]
    pub struct MapInPlace<R, F> {
        #[pin]
        inner: ScanInPlaceWith<R, FuncScanner<F>>,
    }
}

impl<R, F> MapInPlace<R, F> {
    #[inline]
    pub fn new(inner: R, f: F) -> Self {
        Self {
            inner: ScanInPlaceWith::new(inner, FuncScanner::new(f)),
        }
    }
}

impl<R, F> AsyncReadSamples for MapInPlace<R, F>
where
    R: AsyncReadSamples,
    F: FnMut(R::Sample) -> R::Sample,
{
    type Sample = R::Sample;
    type Error = R::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_read_samples(cx, buffer)
    }
}

impl<R, F> GetSampleRate for MapInPlace<R, F>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, F> StreamLength for MapInPlace<R, F>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R, F> FiniteStream for MapInPlace<R, F> where R: FiniteStream {}

pin_project! {
    #[derive(Clone, Copy, Debug)]
    pub struct MapInPlacePod<R, F> {
        #[pin]
        inner: R,
        map: F,
    }
}

impl<R, F> MapInPlacePod<R, F> {
    #[inline]
    pub fn new(inner: R, map: F) -> Self {
        Self { inner, map }
    }
}

impl<R, Q, F> AsyncReadSamples for MapInPlacePod<R, F>
where
    R::Sample: Pod,
    Q: Pod,
    R: AsyncReadSamples,
    F: FnMut(R::Sample) -> Q,
{
    type Sample = Q;
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Q>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        let num_samples_out = buffer.remaining();
        const MIN_BUFFER: usize = 32;

        if num_samples_out == 0 {
            Poll::Ready(Ok(()))
        }
        else if num_samples_out < MIN_BUFFER {
            // fall back to using stack-allocated intermediate buffer
            // otherwise a caller like read_exact will provide smaller and smaller buffers,
            // until this can't use it as an intermediate buffer anymore.
            //
            // however this is only a problem if the input samples are larger than the
            // output samples. we do it in either case here though.
            //
            // and furthermore MIN_BUFFER should not be constant, as this edge case really
            // depends on the size difference and alignment. so this needs fixing someway.
            let mut intermediate_buffer = [<R::Sample as Zeroable>::zeroed(); MIN_BUFFER];
            let mut read_buf = ReadBuf::new(&mut intermediate_buffer[..num_samples_out]);

            match this.inner.poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let num_samples_read_in = read_buf.filled().len();

                    for i in 0..num_samples_read_in {
                        let sample = (this.map)(intermediate_buffer[i]);
                        buffer.unfilled_mut().write_sample(i, sample);
                    }

                    unsafe {
                        buffer.assume_init(num_samples_read_in);
                        buffer.set_filled(buffer.filled().len() + num_samples_read_in);
                    }

                    Poll::Ready(Ok(()))
                }
            }
        }
        else {
            let buffer_initialized = buffer.initialize_unfilled(|| Q::zeroed());
            let (_, buffer_in, _) = bytemuck::pod_align_to_mut::<Q, R::Sample>(buffer_initialized);

            let num_samples_in = buffer_in.len();
            let num_samples = num_samples_out.min(num_samples_in);
            let buffer_in = &mut buffer_in[..num_samples];

            assert!(
                buffer_in.len() > 0,
                "bug: not a single input sample fits into the provided output buffer"
            );

            let mut read_buf = ReadBuf::new(buffer_in);

            match this.inner.poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let num_samples_read_in = read_buf.filled().len();

                    if num_samples_out < num_samples_in {
                        // num_samples_out < num_samples_in
                        // sizeof(sample_out) > sizeof(sample_in)
                        // map reverse
                        for i in (0..num_samples_read_in).rev() {
                            let (_, buffer_in, _) =
                                bytemuck::pod_align_to::<Q, R::Sample>(buffer_initialized);
                            let sample = buffer_in[i];
                            buffer_initialized[i] = (this.map)(sample);
                        }
                    }
                    else {
                        // num_samples_out >= num_samples_in
                        // sizeof(sample_out) =< sizeof(sample_in)
                        // map forward
                        for i in 0..num_samples_read_in {
                            let (_, buffer_in, _) =
                                bytemuck::pod_align_to::<Q, R::Sample>(buffer_initialized);
                            let sample = buffer_in[i];
                            buffer_initialized[i] = (this.map)(sample);
                        }
                    }

                    buffer.set_filled(buffer.filled().len() + num_samples_read_in);

                    Poll::Ready(Ok(()))
                }
            }
        }
    }
}

impl<R, F> GetSampleRate for MapInPlacePod<R, F>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, F> StreamLength for MapInPlacePod<R, F>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R, F> FiniteStream for MapInPlacePod<R, F> where R: FiniteStream {}
