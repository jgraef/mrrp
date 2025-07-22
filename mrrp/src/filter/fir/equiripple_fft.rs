//! [Equiripple FIR filter design by the FFT algorithm][1]
//!
//! [1]: https://yoksis.bilkent.edu.tr/pdf/files/10.1109-79.581378.pdf

use std::sync::Arc;

use num_complex::Complex;
use num_traits::Zero;
use rustfft::{
    Fft,
    FftPlanner,
};

#[derive(Debug, thiserror::Error)]
#[error("equiripple fft algorithm error")]
pub enum Error {
    #[error("produced NAN")]
    NotFinite,
}

#[derive(Clone, Copy, Debug)]
pub struct FrequencyResponseBin {
    pub amplitude: f32,
    pub tolerance: f32,
}

pub trait FrequencyResponse {
    fn len(&self) -> usize;
    fn get(&self, index: usize) -> Option<FrequencyResponseBin>;

    fn iter(&self) -> impl Iterator<Item = Option<FrequencyResponseBin>> {
        (0..self.len()).map(|i| self.get(i))
    }
}

pub struct Algorithm<F> {
    length: usize,
    frequency_response: F,
    h: Vec<Complex<f32>>,
    h_before: Vec<Complex<f32>>,
    fft_forward: Arc<dyn Fft<f32>>,
    fft_inverse: Arc<dyn Fft<f32>>,
    fft_scratch: Vec<Complex<f32>>,
    norm_factor: f32,
}

impl<F> Algorithm<F>
where
    F: FrequencyResponse,
{
    fn new(length: usize, frequency_response: F, fft_planner: &mut FftPlanner<f32>) -> Self {
        assert!(length > 0);
        assert_eq!(length % 2, 1);

        let fft_size = frequency_response.len();
        assert!(fft_size >= length);

        let fft_forward = fft_planner.plan_fft_forward(fft_size);
        let fft_inverse = fft_planner.plan_fft_inverse(fft_size);
        let fft_scratch_size = fft_forward
            .get_inplace_scratch_len()
            .max(fft_inverse.get_inplace_scratch_len());

        Self {
            length,
            frequency_response,
            h: vec![Zero::zero(); fft_size],
            h_before: vec![Zero::zero(); fft_size],
            fft_forward,
            fft_inverse,
            fft_scratch: vec![Default::default(); fft_scratch_size],
            norm_factor: 1.0 / (fft_size as f32),
        }
    }

    pub fn with_h0_estimate(
        length: usize,
        frequency_response: F,
        fft_planner: &mut FftPlanner<f32>,
    ) -> Self {
        let mut this = Self::new(length, frequency_response, fft_planner);
        this.set_h_with_h0_estimate();
        this
    }

    pub fn with_h0(
        length: usize,
        frequency_response: F,
        fft_planner: &mut FftPlanner<f32>,
        h0: &[f32],
    ) -> Self {
        let mut this = Self::new(length, frequency_response, fft_planner);
        this.set_h_from_h0(h0);
        this
    }

    fn set_h_from_h0(&mut self, h0: &[f32]) {
        assert_eq!(h0.len(), self.length);

        for (h, h0) in self
            .h
            .iter_mut()
            .zip(h0.iter().copied().chain(std::iter::repeat(Zero::zero())))
        {
            *h = h0.into();
        }
    }

    fn set_h_with_h0_estimate(&mut self) {
        for (h, h0) in self.h.iter_mut().zip(self.frequency_response.iter()) {
            *h = h0.map_or(Zero::zero(), |response| Complex::from(response.amplitude));
        }
        self.fft_inverse
            .process_with_scratch(&mut self.h, &mut self.fft_scratch);
        self.normalize_and_truncate_h();
    }

    fn normalize_and_truncate_h(&mut self) {
        /*let i0 = self.h.len() - self.length / 2 - 1;
        let i1 = self.length / 2 + 1;

        for h in &mut self.h[..i1] {
            *h *= self.norm_factor;
        }
        for h in &mut self.h[i1 .. i0] {
            *h = Zero::zero();
        }
        for h in &mut self.h[i0..] {
            *h *= self.norm_factor;
        }

        println!("h[i0..] = {:?}", &self.h[i0..]);
        println!("h[..i1] = {:?}", &self.h[..i1]);
        todo!();*/

        for h in &mut self.h[..self.length] {
            *h *= self.norm_factor;
        }
        for h in &mut self.h[self.length..] {
            *h = Zero::zero();
        }
    }

    pub fn step(&mut self) -> Result<f32, Error> {
        self.h_before[..self.length].copy_from_slice(&self.h[..self.length]);

        self.fft_forward
            .process_with_scratch(&mut self.h, &mut self.fft_scratch);

        //panic!("{:#?}", ha);

        for (h, h_id) in self.h.iter_mut().zip(self.frequency_response.iter()) {
            // normalize
            *h *= self.norm_factor;

            if let Some(h_id) = h_id {
                let a = h.norm();
                let a_clamped = a.clamp(
                    h_id.amplitude - h_id.tolerance,
                    h_id.amplitude + h_id.tolerance,
                );
                *h *= a_clamped / a;
            }
        }

        self.fft_inverse
            .process_with_scratch(&mut self.h, &mut self.fft_scratch);

        if !self.h[..self.length].iter().all(|h| h.is_finite()) {
            self.h[..self.length].copy_from_slice(&self.h_before[..self.length]);
            return Err(Error::NotFinite);
        }

        self.normalize_and_truncate_h();

        let mse = self
            .h
            .iter()
            .zip(&self.h_before)
            .take(self.length)
            .map(|(h, h_before)| (h.re - h_before.re).powi(2))
            .sum::<f32>()
            / self.length as f32;

        Ok(mse)
    }

    pub fn h(&self) -> impl Iterator<Item = f32> {
        /*let i0 = self.h.len() - self.length / 2 - 1;
        let i1 = self.length / 2 + 1;
        self.h[..i1].iter().chain(self.h[i0..].iter()).map(|h| h.re)*/

        self.h[..self.length].iter().map(|h| h.re)
    }

    fn force_symmetry(&mut self) {
        let half = self.length / 2;
        for i in 0..half {
            let j = self.length - i - 1;
            let h = 0.5 * (self.h[i] + self.h[j]);
            self.h[i] = h;
            self.h[j] = h;
        }
    }
}

fn estimate_filter_length_for_lowpass_or_highpass(
    transition_bandwidth: f32,
    passband_tolerance: f32,
    stopband_tolerance: f32,
) -> f32 {
    (-20.0 * (passband_tolerance * stopband_tolerance).sqrt().log10() - 13.0)
        / (14.6 * transition_bandwidth)
}

fn default_fft_size(length: usize) -> usize {
    (5 * (length - 1) + 1).next_power_of_two()
}

#[derive(Clone, Copy, Debug)]
pub struct Lowpass {
    pub passband_end: f32,
    pub stopband_start: f32,
    pub passband_tolerance: f32,
    pub stopband_tolerance: f32,
    pub fft_size: usize,
}

impl FrequencyResponse for Lowpass {
    fn len(&self) -> usize {
        self.fft_size
    }

    fn get(&self, index: usize) -> Option<FrequencyResponseBin> {
        /*
        // [0..1] <-> [0..fs]
        let mut f = index as f32 / self.fft_size as f32;

        // this is equivalent to first changing to [-0.5..0.5] and then taking the
        // absolute
        if f > 0.5 {
            f = 1.0 - f;
        }
         */

        let f = if index < self.fft_size / 2 {
            index as f32 / self.fft_size as f32
        }
        else {
            (self.fft_size - 1 - index) as f32 / self.fft_size as f32
        };

        if f <= self.passband_end {
            Some(FrequencyResponseBin {
                amplitude: 1.0,
                tolerance: self.passband_tolerance,
            })
        }
        else if f >= self.stopband_start {
            Some(FrequencyResponseBin {
                amplitude: 0.0,
                tolerance: self.stopband_tolerance,
            })
        }
        else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct FilterDesign {
    pub coefficients: Vec<f32>,
    pub interations: usize,
    pub mean_square_error: f32,
    pub fft_size: usize,
}

pub fn lowpass(
    sample_rate: f32,
    cutoff_frequency: f32,
    transition_bandwidth: f32,
    passband_tolerance: f32,
    stopband_tolerance: f32,
    length: impl Into<Option<usize>>,
    fft_size: impl Into<Option<usize>>,
    mut stop_condition: impl FnMut(usize, f32) -> bool,
) -> Result<FilterDesign, Error> {
    let cutoff_frequency = cutoff_frequency / sample_rate;
    let transition_bandwidth = transition_bandwidth / sample_rate;

    let length = length.into().unwrap_or_else(|| {
        let mut length = estimate_filter_length_for_lowpass_or_highpass(
            transition_bandwidth,
            passband_tolerance,
            stopband_tolerance,
        )
        .ceil() as usize;
        if length % 2 == 0 {
            length += 1;
        }
        length
    });
    dbg!(length);

    let fft_size = fft_size.into().unwrap_or_else(|| default_fft_size(length));
    dbg!(fft_size);

    let b = 0.5 * transition_bandwidth;
    let frequency_response = Lowpass {
        passband_end: cutoff_frequency - b,
        stopband_start: cutoff_frequency + b,
        passband_tolerance,
        stopband_tolerance,
        fft_size,
    };
    dbg!(frequency_response);

    let mut algorithm =
        Algorithm::with_h0_estimate(length, frequency_response, &mut FftPlanner::new());

    let mut i = 0;
    let mut mse;
    loop {
        mse = algorithm.step()?;
        i += 1;
        if stop_condition(i, mse) {
            break;
        }
    }

    Ok(FilterDesign {
        coefficients: algorithm.h().collect(),
        interations: i,
        mean_square_error: mse,
        fft_size,
    })
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use crate::filter::fir::equiripple_fft::{
        estimate_filter_length_for_lowpass_or_highpass,
        lowpass,
    };

    #[test]
    fn it_reproduces_the_example_from_the_paper() {
        let filter_design =
            lowpass(1.0, 0.25, 0.1, 0.05, 0.05, 11, None, |_, e| e < 1.0e-4).unwrap();
        let h = filter_design.coefficients;
        let n = (h.len() - 1) / 2;
        for (i, h) in h.iter().enumerate() {
            println!("{}: {h}", i as isize - n as isize);
        }
        todo!();
    }

    #[test]
    fn it_estimates_lowpass_filter_length_correctly() {
        let length = estimate_filter_length_for_lowpass_or_highpass(0.1, 0.05, 0.05);
        assert_abs_diff_eq!(length, 8.918219118684673);
    }
}
