use std::{
    marker::PhantomData,
    ops::{
        Add,
        Mul,
    },
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
        FiniteStream,
        GetSampleRate,
        ReadBuf,
        Remaining,
        ScratchBuffer,
        StreamLength,
    },
    sample::FromSample,
};

pin_project! {
    /// Stream wrapper that maps the samples using an intermediate buffer.
    #[derive(Clone, Debug)]
    pub struct ScanWith<R, S, Sc> {
        #[pin]
        inner: R,
        scanner: Sc,
        // would be nicer to use SamplesMut, but we need a SamplesMut::drain for that
        // we use super::Buffer here, because it is Clone, we don't need the pointers it keeps track of.
        intermediate_buffer: ScratchBuffer<S>,
        max_buffer_size: usize,
    }
}

impl<R, S, Sc> ScanWith<R, S, Sc> {
    #[inline]
    pub fn new(inner: R, scanner: Sc) -> Self {
        Self {
            inner,
            scanner,
            intermediate_buffer: ScratchBuffer::new(0),
            max_buffer_size: usize::MAX,
        }
    }

    #[inline]
    pub fn with_max_buffer_size(mut self, max_buffer_size: usize) -> Self {
        self.max_buffer_size = max_buffer_size;
        self
    }
}

impl<R, S, Sc> AsyncReadSamples<Sc::Output> for ScanWith<R, S, Sc>
where
    R: AsyncReadSamples<S>,
    Sc: Scanner<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Sc::Output>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        let read_length = (*this.max_buffer_size).min(buffer.remaining());
        this.intermediate_buffer.reserve(read_length);

        let mut read_buf = ReadBuf::uninit(&mut this.intermediate_buffer.buffer[..read_length]);

        match this.inner.poll_read_samples(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let filled = read_buf.filled().len();

                for i in 0..filled {
                    let sample = unsafe { this.intermediate_buffer.buffer[i].assume_init_read() };
                    let sample = this.scanner.scan(sample);

                    buffer.put_sample(sample);
                }

                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<R, S, Sc> GetSampleRate for ScanWith<R, S, Sc>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, S, Sc> StreamLength for ScanWith<R, S, Sc>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R, S, Sc> FiniteStream for ScanWith<R, S, Sc> where R: FiniteStream {}

pin_project! {
    #[derive(Clone, Debug)]
    pub struct ScanInPlaceWith<R, Sc> {
        #[pin]
        inner: R,
        pub scanner: Sc,
    }
}

impl<R, Sc> ScanInPlaceWith<R, Sc> {
    #[inline]
    pub fn new(inner: R, scanner: Sc) -> Self {
        Self { inner, scanner }
    }
}

impl<R, S, Sc> AsyncReadSamples<S> for ScanInPlaceWith<R, Sc>
where
    R: AsyncReadSamples<S>,
    Sc: Scanner<S, Output = S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        let buffer_start = buffer.filled().len();

        match this.inner.poll_read_samples(cx, buffer) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let buffer_end = buffer.filled().len();
                let filled = &mut buffer.inner_mut()[buffer_start..buffer_end];

                for index in 0..filled.len() {
                    let sample_in = unsafe {
                        // SAFETY: we're basically taking samples out of the buffer and
                        // replacing them.
                        filled[index].assume_init_read()
                    };

                    // todo: handle panic in this.map.
                    // so if a panic occurs here the following ranges are initialized
                    // ..index
                    // (index + 1)..buffer.initialized
                    let sample_out = this.scanner.scan(sample_in);

                    filled.write_sample(index, sample_out);
                }

                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<R, Sc> GetSampleRate for ScanInPlaceWith<R, Sc>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, Sc> StreamLength for ScanInPlaceWith<R, Sc>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R, Sc> FiniteStream for ScanInPlaceWith<R, Sc> where R: FiniteStream {}

pub trait Scanner<S> {
    type Output;

    fn scan(&mut self, sample: S) -> Self::Output;
}

impl<T, S> Scanner<S> for &mut T
where
    T: Scanner<S> + ?Sized,
{
    type Output = T::Output;

    #[inline]
    fn scan(&mut self, sample: S) -> Self::Output {
        (&mut **self).scan(sample)
    }
}

impl<T, S> Scanner<S> for Box<T>
where
    T: Scanner<S> + ?Sized,
{
    type Output = T::Output;

    #[inline]
    fn scan(&mut self, sample: S) -> Self::Output {
        (&mut **self).scan(sample)
    }
}

impl<S> Scanner<S> for () {
    type Output = S;

    #[inline]
    fn scan(&mut self, sample: S) -> Self::Output {
        sample
    }
}

pub trait ScannerExt<S>: Scanner<S> {
    #[inline]
    fn chain<T>(self, other: T) -> Chain<Self, T>
    where
        T: Scanner<Self::Output>,
        Self: Sized,
    {
        Chain {
            head: self,
            tail: other,
        }
    }

    #[inline]
    fn map<F, Q>(self, f: F) -> Chain<Self, FuncScanner<F>>
    where
        F: FnMut(Self::Output) -> Q,
        Self: Sized,
    {
        self.chain(FuncScanner::new(f))
    }
}

impl<S, T> ScannerExt<S> for T where T: Scanner<S> {}

#[derive(Clone, Debug)]
pub struct Chain<H, T> {
    head: H,
    tail: T,
}

impl<H, T, S> Scanner<S> for Chain<H, T>
where
    H: Scanner<S>,
    T: Scanner<H::Output>,
{
    type Output = T::Output;

    #[inline]
    fn scan(&mut self, sample: S) -> Self::Output {
        self.tail.scan(self.head.scan(sample))
    }
}

#[derive(Debug)]
pub struct FuncScanner<F> {
    f: F,
}

impl<F> FuncScanner<F> {
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<S, Q, F> Scanner<S> for FuncScanner<F>
where
    F: FnMut(S) -> Q,
{
    type Output = Q;

    #[inline]
    fn scan(&mut self, sample: S) -> Self::Output {
        (self.f)(sample)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConvertScanner<Q> {
    _phantom: PhantomData<fn() -> Q>,
}

impl<Q> ConvertScanner<Q> {
    #[inline]
    pub fn new() -> Self {
        Self {
            _phantom: PhantomData,
        }
    }
}

impl<S, Q> Scanner<S> for ConvertScanner<Q>
where
    Q: FromSample<S>,
{
    type Output = Q;

    #[inline]
    fn scan(&mut self, sample: S) -> Self::Output {
        Q::from_sample(sample)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SumScanner;

impl<S, T> Scanner<(S, T)> for SumScanner
where
    S: Add<T>,
{
    type Output = <S as Add<T>>::Output;

    #[inline]
    fn scan(&mut self, (left, right): (S, T)) -> Self::Output {
        left + right
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProductScanner;

impl<S, T> Scanner<(S, T)> for ProductScanner
where
    S: Mul<T>,
{
    type Output = <S as Mul<T>>::Output;

    #[inline]
    fn scan(&mut self, (left, right): (S, T)) -> Self::Output {
        left * right
    }
}
