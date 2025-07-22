use std::f32::consts::TAU;

use num_complex::Complex;

use crate::{
    GetSampleRate,
    source::SignalGenerator,
};

#[inline]
fn step_from_frequency_and_sample_rate(frequency: f32, sample_rate: f32) -> f32 {
    (TAU * frequency / sample_rate).rem_euclid(TAU)
}

#[derive(Clone, Copy, Debug)]
pub struct SineWave {
    frequency: f32,
    sample_rate: f32,
    phase: f32,
    step: f32,
}

impl SineWave {
    pub fn new(frequency: f32, sample_rate: f32) -> Self {
        Self {
            frequency,
            sample_rate,
            phase: 0.0,
            step: step_from_frequency_and_sample_rate(frequency, sample_rate),
        }
    }

    pub fn with_phase(mut self, phase: f32) -> Self {
        self.phase = phase;
        self
    }

    pub fn set_frequency(&mut self, frequency: f32) {
        self.step = step_from_frequency_and_sample_rate(frequency, self.sample_rate);
    }
}

impl SignalGenerator for SineWave {
    type Sample = f32;

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.step = step_from_frequency_and_sample_rate(self.frequency, sample_rate);
    }

    fn next(&mut self) -> Self::Sample {
        let output = self.phase.sin();
        self.phase += self.step;
        if self.phase > TAU {
            self.phase -= TAU;
        }
        output
    }
}

impl GetSampleRate for SineWave {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ComplexSinusoid {
    frequency: f32,
    sample_rate: f32,
    phase: f32,
    step: f32,
}

impl ComplexSinusoid {
    pub fn new(frequency: f32, sample_rate: f32) -> Self {
        Self {
            frequency,
            sample_rate,
            phase: 0.0,
            step: step_from_frequency_and_sample_rate(frequency, sample_rate),
        }
    }

    pub fn with_phase(mut self, phase: f32) -> Self {
        self.phase = phase;
        self
    }

    pub fn set_frequency(&mut self, frequency: f32) {
        self.step = step_from_frequency_and_sample_rate(frequency, self.sample_rate);
    }
}

impl SignalGenerator for ComplexSinusoid {
    type Sample = Complex<f32>;

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.step = step_from_frequency_and_sample_rate(self.frequency, sample_rate);
    }

    fn next(&mut self) -> Self::Sample {
        let output = Complex {
            im: self.phase,
            re: 0.0,
        }
        .exp();
        self.phase += self.step;
        if self.phase > TAU {
            self.phase -= TAU;
        }
        output
    }
}

impl GetSampleRate for ComplexSinusoid {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

#[inline]
pub fn sine(frequency: f32, sample_rate: f32) -> SineWave {
    SineWave::new(frequency, sample_rate)
}

#[inline]
pub fn complex_sinusoid(frequency: f32, sample_rate: f32) -> ComplexSinusoid {
    ComplexSinusoid::new(frequency, sample_rate)
}
