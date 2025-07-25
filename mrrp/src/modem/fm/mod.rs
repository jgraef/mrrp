use std::f32::consts::TAU;

use num_complex::Complex;
use num_traits::Zero;

use crate::io::Scanner;

/// https://wirelesspi.com/frequency-modulation-fm-and-demodulation-using-dsp-techniques/
#[derive(Clone, Copy, Debug)]
pub struct DifferentiateAndAccessPhase {
    delayed: Complex<f32>,
    norm_factor: f32,
}

impl DifferentiateAndAccessPhase {
    pub fn new(sample_rate: f32, frequency_deviation: f32) -> Self {
        Self {
            delayed: Complex::ZERO,
            norm_factor: sample_rate / (TAU * frequency_deviation),
        }
    }
}

impl Scanner<Complex<f32>> for DifferentiateAndAccessPhase {
    type Output = f32;

    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        let phase_difference = (self.delayed.conj() * sample).arg();
        self.delayed = sample;
        phase_difference * self.norm_factor
    }
}

/// https://wirelesspi.com/frequency-modulation-fm-and-demodulation-using-dsp-techniques/
/// buggy
#[derive(Clone, Copy, Debug)]
pub struct AccessPhaseAndDifferentiate {
    delayed: f32,
    norm_factor: f32,
}

impl AccessPhaseAndDifferentiate {
    pub fn new(sample_rate: f32, frequency_deviation: f32) -> Self {
        Self {
            delayed: 0.0,
            norm_factor: sample_rate / (TAU * frequency_deviation),
        }
    }
}

impl Scanner<Complex<f32>> for AccessPhaseAndDifferentiate {
    type Output = f32;

    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        let phase = sample.arg();
        let phase_difference = phase - self.delayed;
        self.delayed = phase;
        phase_difference * self.norm_factor
    }
}

/// [Slide 12](https://cci.usc.edu/wp-content/uploads/2017/09/CLASS-6-FM-modulation.pdf)
#[derive(Clone, Copy, Debug)]
pub struct DifferentiateAndDivide {
    delay1: Complex<f32>,
    delay2: Complex<f32>,
    norm_factor: f32,
}

impl DifferentiateAndDivide {
    pub fn new(sample_rate: f32, frequency_deviation: f32) -> Self {
        Self {
            delay1: Complex::zero(),
            delay2: Complex::zero(),
            norm_factor: sample_rate / (TAU * frequency_deviation),
        }
    }
}

impl Scanner<Complex<f32>> for DifferentiateAndDivide {
    type Output = f32;

    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        let output = if self.delay1.is_zero() {
            // don't divide by 0
            0.0
        }
        else {
            let a = (sample.re - self.delay2.re) * self.delay1.im;
            let b = (sample.im - self.delay2.im) * self.delay1.re;
            (b - a) / self.delay1.norm_sqr() * self.norm_factor
        };

        self.delay2 = self.delay1;
        self.delay1 = sample;

        output
    }
}

pub type FmDemodulator = DifferentiateAndDivide;

#[derive(Clone, Copy, Debug)]
pub struct FmModulator {
    delay: f32,
    frequency_modulation_factor: f32,
    carrier_frequency: f32,
}

impl FmModulator {
    pub fn new(sample_rate: f32, frequency_deviation: f32) -> Self {
        Self {
            delay: 0.0,
            frequency_modulation_factor: sample_rate / (TAU * frequency_deviation),
            carrier_frequency: 0.0,
        }
    }
}

impl Scanner<f32> for FmModulator {
    type Output = Complex<f32>;

    fn scan(&mut self, sample: f32) -> Self::Output {
        let f = self.delay + self.frequency_modulation_factor * sample;
        self.delay = f;
        Complex {
            re: 0.0,
            im: f + self.carrier_frequency,
        }
        .exp()
    }
}
