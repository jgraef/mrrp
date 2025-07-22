pub mod biquad;
pub mod resampling;

use std::{
    collections::VecDeque,
    fmt::Debug,
    ops::{
        Add,
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
    Float,
    FloatConst,
    FromPrimitive,
    Zero,
};

use crate::{
    GetSampleRate,
    io::{
        AsyncReadSamples,
        ReadBuf,
        Scanner,
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
pub struct FirFilter<S, C> {
    coefficients: Vec<C>,
    delayed: VecDeque<S>,
}

impl<S, C> FirFilter<S, C> {
    #[inline]
    pub fn new(coefficients: Vec<C>) -> Self {
        assert!(coefficients.len() > 0);

        let delayed = VecDeque::with_capacity(coefficients.len() - 1);

        Self {
            coefficients,
            delayed,
        }
    }

    #[inline]
    pub fn order(&self) -> usize {
        self.coefficients.len() - 1
    }
}

impl<S, C> FirFilter<S, C>
where
    C: Float + FloatConst + FromPrimitive,
{
    pub fn hann(_sample_frequency: f32, _cutoff_frequency: f32) -> Self {
        //Self::new(hann_window(order).collect())
        todo!();
    }
}

impl<S, C> Scanner<S> for FirFilter<S, C>
where
    S: Copy + Mul<C, Output = S> + Add<S, Output = S>,
    C: Copy,
{
    type Output = S;

    fn scan(&mut self, sample: S) -> Self::Output {
        debug_assert!(self.delayed.len() < self.coefficients.len());

        let mut output = sample * self.coefficients[0];
        for (delayed, coeff) in self.delayed.iter().zip(&self.coefficients[1..]) {
            output = sample + *delayed * *coeff;
        }

        if self.delayed.len() == self.coefficients.len() - 1 {
            self.delayed.pop_back();
        }
        self.delayed.push_front(sample);

        output
    }
}

// I wanted to implement a fast convolution on the delayed buffer and read
// buffer, but it got too complicated lol
#[allow(dead_code)]
fn convolve_delayed<S, C>(coeffients: &[C], delayed: &mut Vec<S>, read: &mut [S]) -> usize
where
    S: Sample + Zero + AddAssign,
    C: Copy + Mul<S, Output = S>,
{
    assert!(delayed.len() < coeffients.len());

    // if we're missing samples in the delay buffer, we need to copy them over,
    // because the read buffer will be overwritten.
    let missing_in_delay = coeffients.len().saturating_sub(delayed.len() + 1);
    delayed.extend_from_slice(&read[..missing_in_delay]);

    let read_start = missing_in_delay;

    let mut i = 0;

    while i < delayed.len() {
        let mut s = S::zero();
        let mut c = coeffients.len() - 1;

        for j in i..delayed.len() {
            s += coeffients[c] * delayed[j];
            c -= 1;
        }

        for j in 0..=i {
            s += coeffients[c] * read[read_start + j];
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
            s += coeffients[c] * read[read_start + j];
            c -= 1;
        }

        read[i] = s;

        i += 1;
        i0 += 1;
    }

    n
}

pub fn hann_window<T>(n: usize) -> impl Iterator<Item = T>
where
    T: Float + FloatConst + FromPrimitive,
{
    let n_t = T::from_usize(n).unwrap();
    (0..=n).map(move |i| (T::PI() * T::from_usize(i).unwrap() / n_t).sin().powi(2))
}
