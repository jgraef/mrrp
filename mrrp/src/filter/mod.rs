pub mod biquad;
pub mod design;
pub mod fir;
pub mod resampling;

use std::{
    fmt::Debug,
    ops::{
        AddAssign,
        Div,
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

use crate::{
    GetSampleRate,
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
