#[cfg(feature = "biquad")]
pub mod biquad;
pub mod design;
pub mod fir;
pub mod resampling;

use std::{
    collections::VecDeque,
    f32::consts::TAU,
    fmt::Debug,
    ops::{
        AddAssign,
        Mul,
        SubAssign,
    },
};

use mrrp_core::signal::{
    AsyncReadSamples,
    GetSampleRate,
    combinators::Scanner,
};
use num_complex::Complex;
use num_traits::Zero;

#[cfg(feature = "pm-remez")]
pub use crate::design::pm_remez::pm_remez;
#[cfg(feature = "pm-remez")]
use crate::design::{
    FilterDesign,
    Hilbert,
    Normalize,
};
use crate::{
    fir::FirFilter,
    resampling::{
        Decimate,
        Interpolate,
    },
};

pub trait MakeFilter<R> {
    type Filter;

    fn make_filter(&self, input: &R) -> Self::Filter;
}

/// Hilbert filter to recover an IQ signal from a real-valued signal
///
/// # TODO
///
/// The constructor requires the `pm-remez` filter to construct a filter. Is
/// this really necesarry?
#[derive(Clone, Debug)]
pub struct HilbertFilter {
    hilbert: FirFilter<f32, f32>,
}

impl HilbertFilter {
    #[cfg(feature = "pm-remez")]
    pub fn new(transition_bandwidth: f32, filter_length: usize) -> Self {
        assert!(filter_length % 2 == 1, "filter length must be odd");

        // why does this use pm-remez again?
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

#[derive(Clone, Debug)]
pub struct MovingAverage<S> {
    length: usize,
    norm: f32,
    sum: S,
    delay: VecDeque<S>,
}

impl<S> MovingAverage<S>
where
    S: Zero,
{
    pub fn new(length: usize) -> Self {
        assert!(length > 0);
        Self {
            length,
            norm: 1.0 / (length as f32),
            sum: Zero::zero(),
            delay: VecDeque::with_capacity(length),
        }
    }
}

impl<S> Scanner<S> for MovingAverage<S>
where
    S: Copy + AddAssign<S> + SubAssign<S> + Mul<f32, Output = S>,
{
    type Output = S;

    fn scan(&mut self, sample: S) -> Self::Output {
        self.sum += sample;

        if self.delay.len() == self.length {
            let old = self.delay.pop_front().unwrap();
            self.sum -= old;
        }
        self.delay.push_back(sample);

        self.sum * self.norm
    }
}

pub trait AsyncReadSamplesFilterExt<S>: AsyncReadSamples<S> {
    /// Decimate the input stream
    ///
    /// The returned stream will only return one out of `factor` samples and
    /// drop the rest.
    #[inline]
    fn decimate(self, factor: usize) -> Decimate<Self>
    where
        Self: Sized,
    {
        Decimate::new(self, factor)
    }

    /// Decimate the input stream to a target sample rate
    ///
    /// This will decimate such that the resulting sample rate is as close as
    /// possible to the `target_sampling_rate`.
    ///
    /// This is a short-hand for:
    ///
    /// ```
    /// # use mrrp_core::signal::{combinators::WithSampleRate, Silence, silence, AsyncReadSamplesExt, GetSampleRate};
    /// # use mrrp_filter::AsyncReadSamplesFilterExt;
    /// # fn main() {
    /// # let target_sample_rate = 10.0;
    /// # let input: WithSampleRate<Silence<f32>> = silence().with_sample_rate(100.0);
    /// let sample_rate = input.sample_rate();
    /// let decimated = input.decimate((sample_rate / target_sample_rate).round() as usize);
    /// # }
    /// ```
    ///
    /// # TODO
    ///
    /// We can probably make this exact by alternating between the floored and
    /// ceiled decimation rate.
    #[inline]
    fn decimate_to(self, target_sample_rate: f32) -> Decimate<Self>
    where
        Self: Sized + GetSampleRate,
    {
        let sample_rate = self.sample_rate();
        self.decimate((sample_rate / target_sample_rate).round() as usize)
    }

    /// Interpolate the input stream
    ///
    /// This pads the input stream such that one input sample is followed by
    /// `factor - 1` samples containing silence.
    #[inline]
    fn interpolate(self, factor: usize) -> Interpolate<Self>
    where
        Self: Sized,
    {
        Interpolate::new(self, factor)
    }

    /// Interpolate the input stream to a target sample rate
    ///
    /// This will interpolate such that the resulting sample rate is as close as
    /// possible to the `target_sampling_rate`.
    ///
    /// This is a short-hand for:
    ///
    /// ```
    /// # use mrrp_core::signal::{combinators::WithSampleRate, Silence, silence, AsyncReadSamplesExt, GetSampleRate};
    /// # use mrrp_filter::AsyncReadSamplesFilterExt;
    /// # fn main() {
    /// # let target_sample_rate = 1000.0;
    /// # let input: WithSampleRate<Silence<f32>> = silence().with_sample_rate(100.0);
    /// let sample_rate = input.sample_rate();
    /// let interpolated = input.interpolate((target_sample_rate / sample_rate).round() as usize);
    /// # }
    /// ```
    ///
    /// # TODO
    ///
    /// We can probably make this exact by alternating between the floored and
    /// ceiled interpolation rate.
    #[inline]
    fn interpolate_to(self, target_sample_rate: f32) -> Interpolate<Self>
    where
        Self: Sized + GetSampleRate,
    {
        let sample_rate = self.sample_rate();
        self.interpolate((target_sample_rate / sample_rate).round() as usize)
    }
}

impl<R, S> AsyncReadSamplesFilterExt<S> for R where R: AsyncReadSamples<S> + ?Sized {}
