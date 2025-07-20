//!
//! # TODO
//!
//! CIC filter: <https://www.researchgate.net/publication/228648728_Understanding_cascaded_integrator-comb_filters>

pub mod biquad;
pub mod synthesis;

use std::{
    collections::VecDeque,
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
};

#[derive(Clone, Debug)]
pub struct DelayLine<S> {
    buffer: VecDeque<S>,
    length: usize,
}

impl<S> DelayLine<S> {
    #[inline]
    pub fn new(length: usize) -> Self {
        assert!(length > 0);
        Self {
            buffer: VecDeque::with_capacity(length),
            length,
        }
    }

    #[inline]
    pub fn push(&mut self, sample: S) -> Option<S> {
        assert!(self.buffer.len() <= self.length);
        let delayed = (self.buffer.len() == self.length).then(|| self.buffer.pop_front().unwrap());
        self.buffer.push_back(sample);
        delayed
    }

    #[inline]
    pub fn get(&mut self, age: usize) -> Option<&S> {
        let index = self.length.checked_sub(age + 1)?;
        self.buffer.get(index)
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
        match this.input.poll_read_samples(cx, buffer) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let mut read_pos = 0;
                let mut write_pos = 0;

                let mut num_samples = buffer.filled().len();
                //let filled_uninit = buffer.inner_mut()[..num_samples];
                todo!();
            }
        }
    }
}
