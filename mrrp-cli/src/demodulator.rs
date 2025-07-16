use std::{
    collections::VecDeque,
    f32::consts::TAU,
    sync::Arc,
};

use num_complex::Complex;
use parking_lot::Mutex;
use serde::{
    Deserialize,
    Serialize,
};

use crate::util::FrequencyBand;

#[derive(Debug)]
pub struct Demodulator {
    shift: ComplexSine,
    lowpass: FirFilter,
    //lowpass: biquad::DirectForm1<f32>,
    decimation: usize,
    next_decimation: usize,
    audio_buffer: Arc<Mutex<AudioBuffer>>,
    audio_source: AudioSource,
}

impl Demodulator {
    pub fn new(frequency_band: FrequencyBand, sampled_frequency_band: FrequencyBand) -> Self {
        let shift = frequency_band.center() as f32 - sampled_frequency_band.center() as f32;
        let shift = ComplexSine::new(-shift, sampled_frequency_band.bandwidth() as f32);

        let decimation = sampled_frequency_band
            .bandwidth()
            .div_ceil(frequency_band.bandwidth() / 2) as usize;

        let lowpass = FirFilter::boxcar(decimation + 1);

        /*let lowpass = biquad::DirectForm1::new(
            biquad::Coefficients::from_params(
                biquad::Type::LowPass,
                sampled_frequency_band.bandwidth().hz(),
                (frequency_band.bandwidth() / 2).hz(),
                biquad::Q_BUTTERWORTH_F32,
            )
            .unwrap(),
        );*/

        let audio_buffer = Arc::new(Mutex::new(AudioBuffer::new(
            frequency_band.bandwidth() as usize
        )));
        let audio_source = AudioSource {
            audio_buffer: audio_buffer.clone(),
            sample_rate: frequency_band.bandwidth() / 2,
        };

        Self {
            shift,
            lowpass,
            decimation: decimation,
            next_decimation: 0,
            audio_buffer,
            audio_source,
        }
    }

    pub fn audio_source(&mut self) -> AudioSource {
        self.audio_source.clone()
    }

    pub fn push(&mut self, input: &[Complex<f32>]) {
        let mut audio_buffer = self.audio_buffer.lock();

        for sample in input {
            let sample = *sample * self.shift.sample();
            self.lowpass.push(sample);
            //let sample = self.lowpass.run(sample.norm());

            if self.next_decimation == 0 {
                audio_buffer.push(self.lowpass.sample().norm());
                //audio_buffer.push(sample);

                self.next_decimation = self.decimation - 1;
            }
            else {
                self.next_decimation -= 1;
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FirFilter {
    buffer: VecDeque<Complex<f32>>,
    coefficients: Vec<f32>,
}

impl FirFilter {
    pub fn boxcar(order: usize) -> Self {
        Self {
            buffer: Default::default(),
            coefficients: vec![1.0 / (order as f32 + 1.0); order + 1],
        }
    }

    pub fn push(&mut self, sample: Complex<f32>) {
        while self.buffer.len() + 1 > self.coefficients.len() {
            self.buffer.pop_back();
        }
        self.buffer.push_front(sample);
    }

    pub fn sample(&self) -> Complex<f32> {
        self.coefficients
            .iter()
            .zip(&self.buffer)
            .map(|(coefficient, signal)| coefficient * signal)
            .sum()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComplexSine {
    f: f32,
    t: f32,
    dt: f32,
}

impl ComplexSine {
    pub fn new(frequency: f32, sample_rate: f32) -> Self {
        Self {
            f: frequency,
            t: 0.0,
            dt: 1.0 / sample_rate,
        }
    }

    pub fn sample(&mut self) -> Complex<f32> {
        let y = (-Complex::i() * TAU * self.f * self.t).exp();
        self.t += self.dt;
        y
    }
}

#[derive(Debug)]
struct AudioBuffer {
    buffer: VecDeque<f32>,
    buffer_size: usize,
}

impl AudioBuffer {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer: VecDeque::with_capacity(buffer_size),
            buffer_size,
        }
    }

    pub fn push(&mut self, sample: f32) {
        while self.buffer.len() + 1 > self.buffer_size {
            self.buffer.pop_front();
        }

        self.buffer.push_back(sample);
    }

    pub fn next(&mut self) -> Option<f32> {
        self.buffer.pop_front()
    }
}

#[derive(Clone, Debug)]
pub struct AudioSource {
    audio_buffer: Arc<Mutex<AudioBuffer>>,
    sample_rate: u32,
}

impl rodio::Source for AudioSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> rodio::ChannelCount {
        1
    }

    fn sample_rate(&self) -> rodio::SampleRate {
        self.sample_rate
    }

    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}

impl Iterator for AudioSource {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<Self::Item> {
        let mut audio_buffer = self.audio_buffer.lock();
        Some(audio_buffer.next().unwrap_or_default())
    }
}
