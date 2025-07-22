pub mod equiripple_fft;

use std::{
    collections::VecDeque,
    ops::{
        Add,
        AddAssign,
        Mul,
    },
};

use num_traits::{
    Float,
    FloatConst,
    FromPrimitive,
    Zero,
};

use crate::{
    io::Scanner,
    sample::Sample,
};

#[derive(Clone, Debug)]
pub struct FirFilter<S, C> {
    coefficients: Vec<C>,
    delayed: VecDeque<S>,
}

impl<S, C> FirFilter<S, C> {
    #[inline]
    pub fn new(coefficients: Vec<C>) -> Self {
        assert!(coefficients.len() > 1);

        let delayed = VecDeque::with_capacity(coefficients.len() - 1);

        Self {
            coefficients,
            delayed,
        }
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
            output = output + *delayed * *coeff;
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
