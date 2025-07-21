pub mod biquad;
pub mod synthesis;

use std::{
    fmt::Debug,
    ops::{
        AddAssign,
        Div,
        Mul,
    },
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use num_traits::{
    FromPrimitive,
    Zero,
};
use pin_project_lite::pin_project;

use crate::{
    GetSampleRate,
    buf::{
        SampleBufMut,
        SamplesMut,
    },
    io::{
        AsyncReadSamples,
        ReadBuf,
    },
    sample::Sample,
    util::dim::{
        Const,
        DequeLike,
        Dim,
        Dyn,
    },
};

#[derive(Clone, Debug)]
pub struct DelayLine<D: Dim, S> {
    buffer: D::BoundedDeque<S>,
}

impl<const DIM: usize, S> Default for DelayLine<Const<DIM>, S> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<const DIM: usize, S> DelayLine<Const<DIM>, S> {
    #[inline]
    pub fn new() -> Self {
        Self::from_dim(Const::<DIM>)
    }
}

impl<S> DelayLine<Dyn, S> {
    #[inline]
    pub fn new(dimension: usize) -> Self {
        Self::from_dim(Dyn(dimension))
    }
}

impl<D: Dim, S> DelayLine<D, S> {
    #[inline]
    pub fn from_dim(dim: D) -> Self {
        Self {
            buffer: <D::BoundedDeque<S>>::new(dim),
        }
    }

    #[inline]
    pub fn push(&mut self, sample: S) -> Option<S> {
        self.buffer.push_back(sample)
    }

    #[inline]
    pub fn get(&mut self, age: usize) -> Option<&S> {
        let index = self.buffer.len().checked_sub(age + 1)?;
        self.buffer.get(index)
    }
}

pub struct Coefficients<D: Dim, T>(pub <D as Dim>::Array<T>);

impl<D: Dim, T> Clone for Coefficients<D, T>
where
    <D as Dim>::Array<T>: Clone,
{
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<D: Dim, T> Debug for Coefficients<D, T>
where
    <D as Dim>::Array<T>: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Coefficients").field(&self.0).finish()
    }
}

/// A simple averaging low-pass filter for decimation.
///
/// This takes the average of N samples to produce one sample. It is equivalent
/// to a FIR filter with N coefficients of 1/N followed by a decimation of N
/// samples to 1.
///
/// <https://en.wikipedia.org/wiki/Finite_impulse_response#Moving_average_example>
#[derive(Clone, Debug)]
pub struct AverageDecimate<T, S> {
    input: T,
    decimate: usize,
    accumulator: (S, usize),
}

impl<T, S> AverageDecimate<T, S>
where
    S: Zero,
{
    #[inline]
    pub fn new(input: T, decimate: usize) -> Self {
        Self {
            input,
            decimate,
            accumulator: (S::zero(), 0),
        }
    }
}

impl<T, S> GetSampleRate for AverageDecimate<T, S>
where
    T: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.input.sample_rate() / self.decimate as f32
    }
}

impl<T, S> AsyncReadSamples<S> for AverageDecimate<T, S>
where
    T: AsyncReadSamples<S> + Unpin,
    S: Sample + Clone + Zero + AddAssign + Div<Output = S> + Unpin + Div<S::Scalar, Output = S>,
    S::Scalar: FromPrimitive,
{
    type Error = T::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = &mut *self;

        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        loop {
            match Pin::new(&mut this.input).poll_read_samples(cx, buffer) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let mut write_pos = 0;

                    for read_pos in 0..buffer.filled().len() {
                        this.accumulator.0 += buffer.filled()[read_pos].clone();
                        this.accumulator.1 += 1;
                        if this.accumulator.1 == this.decimate {
                            let average = std::mem::replace(&mut this.accumulator.0, S::zero())
                                / <S::Scalar>::from_usize(this.decimate).unwrap();
                            buffer.filled_mut()[write_pos] = average;
                            write_pos += 1;
                            this.accumulator.1 = 0;
                        }
                    }

                    buffer.set_filled(write_pos);

                    if write_pos > 0 {
                        return Poll::Ready(Ok(()));
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FirFilter<T, S, C> {
    pub input: T,
    pub coefficients: Vec<C>,
    pub delayed: SamplesMut<S>,
}

impl<T, S, C> AsyncReadSamples<S> for FirFilter<T, S, C>
where
    S: Copy,
    T: AsyncReadSamples<S> + Unpin,
    S: Clone + Zero + AddAssign + Unpin,
    C: Unpin,
    for<'a> &'a C: Mul<&'a S, Output = S>,
{
    type Error = T::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = &mut *self;

        match Pin::new(&mut this.input).poll_read_samples(cx, buffer) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let read = buffer.filled_mut();
                convolve_delayed(&this.coefficients, &mut this.delayed, read);
                todo!();
            }
        }
    }
}

fn convolve_delayed<S, C>(coeffients: &[C], delayed: &mut SamplesMut<S>, read: &mut [S]) -> usize
where
    S: Clone + Zero + AddAssign,
    for<'a> &'a C: Mul<&'a S, Output = S>,
{
    assert!(delayed.len() < coeffients.len());

    // if we're missing samples in the delay buffer, we need to copy them over,
    // because the read buffer will be overwritten.
    let missing_in_delay = coeffients.len().saturating_sub(delayed.len() + 1);
    delayed.put(&read[..missing_in_delay]);

    let read_start = missing_in_delay;

    let mut i = 0;

    while i < delayed.len() {
        let mut s = S::zero();
        let mut c = coeffients.len() - 1;

        for j in i..delayed.len() {
            s += &coeffients[c] * &delayed[j];
            c -= 1;
        }

        for j in 0..=i {
            s += &coeffients[c] * &read[read_start + j];
            c -= 1;
        }

        read[i] = s;

        i += 1;
    }

    let mut i0 = 0;
    let n = read.len() - missing_in_delay;
    assert_eq!(i - i0 + 1, coeffients.len());

    while i < n {
        let mut s = S::zero();
        let mut c = coeffients.len() - 1;

        for j in i0..=i {
            s += &coeffients[c] * &read[read_start + j];
            c -= 1;
        }

        read[i] = s;

        i += 1;
        i0 += 1;
    }

    n
}

pin_project! {
    #[derive(Clone, Debug)]
    pub struct Decimate<R> {
        #[pin]
        input: R,
        decimate: usize,
        counter: usize,
    }
}

impl<R> Decimate<R> {
    pub fn new(input: R, decimate: usize) -> Self {
        Self {
            input,
            decimate,
            counter: 0,
        }
    }
}

impl<R, S> AsyncReadSamples<S> for Decimate<R>
where
    R: AsyncReadSamples<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        let mut read_buf = ReadBuf::uninit(buffer.unfilled_mut());

        match this.input.poll_read_samples(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let mut read_pos = 0;
                let mut write_pos = 0;

                let num_samples = read_buf.filled().len();
                let buffer_uninit = read_buf.inner_mut();

                while read_pos < num_samples {
                    // note: we drop all the samples that are not written back

                    let sample = unsafe { buffer_uninit[read_pos].assume_init_read() };
                    read_pos += 1;

                    if *this.counter == 0 {
                        buffer_uninit.write_sample(write_pos, sample);
                        write_pos += 1;
                    }

                    *this.counter += 1;
                    if this.counter == this.decimate {
                        *this.counter = 0;
                    }
                }

                unsafe {
                    buffer.assume_init(write_pos);
                }
                buffer.set_filled(buffer.filled().len() + write_pos);

                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<R> GetSampleRate for Decimate<R>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.input.sample_rate() / self.decimate as f32
    }
}
