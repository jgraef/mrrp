use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytemuck::Pod;
use pin_project_lite::pin_project;

use crate::io::{
    AsyncReadSamples,
    GetSampleRate,
    ReadBuf,
    StreamLength,
    combinators::scan::{
        FuncScanner,
        ScanInPlaceWith,
        ScanWith,
    },
};

pin_project! {
    /// Stream wrapper that maps the samples using an intermediate buffer.
    #[derive(Debug)]
    pub struct Map<R, S, F> {
        #[pin]
        inner: ScanWith<R, S, FuncScanner<F>>,
    }
}

impl<R, S, F> Map<R, S, F> {
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

impl<R, S, Q, F> AsyncReadSamples<Q> for Map<R, S, F>
where
    R: AsyncReadSamples<S>,
    F: FnMut(S) -> Q,
{
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

impl<R, S, F> GetSampleRate for Map<R, S, F>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, S, F> StreamLength for Map<R, S, F>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}

pin_project! {
    /// Stream wrapper that maps the samples using an intermediate buffer.
    #[derive(Debug)]
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

impl<R, S, F> AsyncReadSamples<S> for MapInPlace<R, F>
where
    R: AsyncReadSamples<S>,
    F: FnMut(S) -> S,
{
    type Error = R::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
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
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}

pin_project! {
    #[derive(Clone, Copy, Debug)]
    pub struct MapInPlacePod<R, S, F> {
        #[pin]
        inner: R,
        map: F,
        _phantom: PhantomData<fn(S)>,
    }
}

impl<R, S, F> MapInPlacePod<R, S, F> {
    #[inline]
    pub fn new(inner: R, map: F) -> Self {
        Self {
            inner,
            map,
            _phantom: PhantomData,
        }
    }
}

impl<R, S, Q, F> AsyncReadSamples<Q> for MapInPlacePod<R, S, F>
where
    S: Pod,
    Q: Pod,
    R: AsyncReadSamples<S>,
    F: FnMut(S) -> Q,
{
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
            let mut intermediate_buffer = [S::zeroed(); MIN_BUFFER];
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
            let (_, buffer_in, _) = bytemuck::pod_align_to_mut::<Q, S>(buffer_initialized);

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
                                bytemuck::pod_align_to::<Q, S>(buffer_initialized);
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
                                bytemuck::pod_align_to::<Q, S>(buffer_initialized);
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

impl<R, S, F> GetSampleRate for MapInPlacePod<R, S, F>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, S, F> StreamLength for MapInPlacePod<R, S, F>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}
