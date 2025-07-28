pub mod biquad;
pub mod design;
pub mod fir;
pub mod resampling;

use std::{
    f32::consts::TAU,
    fmt::Debug,
};

use num_complex::Complex;

use crate::{
    filter::{
        design::{
            FilterDesign,
            Hilbert,
            Normalize,
            pm_remez::pm_remez,
        },
        fir::FirFilter,
    },
    io::combinators::Scanner,
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

pub trait MakeFilter<R> {
    type Filter;

    fn make_filter(&self, input: &R) -> Self::Filter;
}

/// Hilbert filter to recover an IQ signal from a real-valued signal
#[derive(Clone, Debug)]
pub struct HilbertFilter {
    hilbert: FirFilter<f32, f32>,
}

impl HilbertFilter {
    pub fn new(transition_bandwidth: f32, filter_length: usize) -> Self {
        assert!(filter_length % 2 == 1, "filter length must be odd");
        let hilbert = pm_remez(
            Hilbert::new(transition_bandwidth).assert_normalized(),
            filter_length,
        )
        .expect("failed to design hilbert filter");
        Self {
            hilbert: hilbert.fir_filter(),
        }
    }
}

impl Scanner<f32> for HilbertFilter {
    type Output = Complex<f32>;

    fn scan(&mut self, sample: f32) -> Self::Output {
        let q = self.hilbert.scan(sample);
        Complex { re: sample, im: q }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GoertzelFilter {
    exp_filter_frequency: Complex<f32>,
    s: [Complex<f32>; 2],
    i: usize,
    n: usize,
    y: Complex<f32>,
    norm: f32,
}

impl GoertzelFilter {
    pub fn new(sample_rate: f32, filter_frequency: f32, filter_bandwidth: f32) -> Self {
        let n = (sample_rate / filter_bandwidth) as usize;
        Self {
            exp_filter_frequency: (-Complex::i() * filter_frequency / sample_rate * TAU).exp(),
            s: Default::default(),
            i: 0,
            n,
            y: Default::default(),
            norm: 1.0, //TAU * filter_bandwidth / filter_frequency,
        }
    }
}

impl Scanner<Complex<f32>> for GoertzelFilter {
    type Output = Complex<f32>;

    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        let s = sample + 2.0 * self.exp_filter_frequency.re * self.s[0] - self.s[1];
        let y = s - self.exp_filter_frequency * self.s[0];
        self.s[1] = self.s[0];
        self.s[0] = s;

        self.i += 1;
        if self.i == self.n {
            self.i = 0;
            self.y = y;
            self.s = Default::default();
        }
        self.y * self.norm
    }
}
