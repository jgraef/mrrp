//! [Equiripple FIR filter design by the FFT algorithm][1]
//!
//! There's an example [here][2].
//!
//! [1]: https://yoksis.bilkent.edu.tr/pdf/files/10.1109-79.581378.pdf
//! [2]: https://www.recordingblogs.com/wiki/equiripple-filter

use std::sync::Arc;

use num_complex::Complex;
use num_traits::Zero;
use rustfft::{
    Fft,
    FftPlanner,
};

use crate::filter::design::{
    DesiredFrequencyResponse,
    SampledIdealFrequencyResponse,
    ToConcreteFilterLength,
    fft_size_for_filter_length,
};

#[derive(Debug, thiserror::Error)]
#[error("equiripple fft algorithm error")]
pub enum Error {
    #[error("produced NAN")]
    NotFinite,
    #[error("filter length not specified")]
    FilterLengthNotSpecified,
}

pub struct Algorithm<S> {
    length: usize,
    ideal_frequency_response: SampledIdealFrequencyResponse<S>,
    h: Vec<Complex<f32>>,
    h_before: Vec<Complex<f32>>,
    fft_forward: Arc<dyn Fft<f32>>,
    fft_inverse: Arc<dyn Fft<f32>>,
    fft_scratch: Vec<Complex<f32>>,
    norm_factor: f32,
}

impl<S> Algorithm<S>
where
    S: DesiredFrequencyResponse,
{
    fn new(
        length: usize,
        ideal_frequency_response: SampledIdealFrequencyResponse<S>,
        fft_planner: &mut FftPlanner<f32>,
    ) -> Self {
        assert!(length > 0);
        assert_eq!(length % 2, 1);

        let fft_size = ideal_frequency_response.fft_size;
        assert!(fft_size >= length);

        let fft_forward = fft_planner.plan_fft_forward(fft_size);
        let fft_inverse = fft_planner.plan_fft_inverse(fft_size);
        let fft_scratch_size = fft_forward
            .get_inplace_scratch_len()
            .max(fft_inverse.get_inplace_scratch_len());

        Self {
            length,
            ideal_frequency_response,
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
        ideal_frequency_response: SampledIdealFrequencyResponse<S>,
        fft_planner: &mut FftPlanner<f32>,
    ) -> Self {
        let mut this = Self::new(length, ideal_frequency_response, fft_planner);
        this.set_h_with_h0_estimate();
        this
    }

    pub fn with_h0(
        length: usize,
        ideal_frequency_response: SampledIdealFrequencyResponse<S>,
        fft_planner: &mut FftPlanner<f32>,
        h0: &[f32],
    ) -> Self {
        let mut this = Self::new(length, ideal_frequency_response, fft_planner);
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
        for (_i, (h, h0)) in self
            .h
            .iter_mut()
            .zip(self.ideal_frequency_response.iter())
            .enumerate()
        {
            // todo: is this needed? no appararent changes to result
            // phase shift for delay by half the samples
            //let m = (self.frequency_response.fft_size() / 2  - 1) as f32;
            //let w = self.frequency_response.frequency(i);
            //let p = (Complex::i() * TAU * w * m).exp();

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
            //h.im = 0.0;
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

        for (h, h_id) in self.h.iter_mut().zip(self.ideal_frequency_response.iter()) {
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

    pub fn h(&self) -> impl Iterator<Item = Complex<f32>> {
        /*let i0 = self.h.len() - self.length / 2 - 1;
        let i1 = self.length / 2 + 1;
        self.h[..i1].iter().chain(self.h[i0..].iter()).map(|h| h.re)*/

        //self.h[..self.length].iter().map(|h| h.re)
        self.h[..self.length].iter().copied()
    }
}

#[derive(Clone, Debug)]
pub struct FilterDesign {
    pub coefficients: Vec<Complex<f32>>,
    pub interations: usize,
    pub mean_square_error: f32,
    pub fft_size: usize,
}

pub fn equiripple_fft<S>(
    filter_specification: S,
    length: impl ToConcreteFilterLength<S>,
    fft_size: impl Into<Option<usize>>,
    mut stop_condition: impl FnMut(usize, f32) -> bool,
) -> Result<FilterDesign, Error>
where
    S: DesiredFrequencyResponse,
{
    let length = length.to_concrete_filter_length(&filter_specification);
    dbg!(length);

    let fft_size = fft_size
        .into()
        .unwrap_or_else(|| fft_size_for_filter_length(length));

    let mut algorithm = Algorithm::with_h0_estimate(
        length,
        filter_specification.sampled(fft_size),
        &mut FftPlanner::new(),
    );

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

    use super::equiripple_fft;
    use crate::filter::design::{
        Lowpass,
        Normalize,
    };

    #[test]
    fn it_reproduces_the_example_from_the_paper() {
        let filter_design = equiripple_fft(
            Lowpass::new(0.25, 0.1, 0.05, 0.05).assert_normalized(),
            11,
            None,
            |_, e| e < 1.0e-4,
        )
        .unwrap();

        let h = filter_design.coefficients;
        let n = (h.len() - 1) / 2;
        for (i, h) in h.iter().enumerate() {
            println!("{}: {h}", i as isize - n as isize);
        }
        todo!();
    }
}
