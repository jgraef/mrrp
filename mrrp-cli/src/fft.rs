use std::{
    f32::consts::PI,
    fmt::Debug,
    str::FromStr,
    sync::Arc,
};

use color_eyre::eyre::eyre;
use num_complex::Complex;
use rustfft::FftPlanner;

use crate::Error;

pub struct Fft {
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    window: Vec<f32>,
    fft: Arc<dyn rustfft::Fft<f32>>,
    size: usize,
    normalization: f32,
}

impl Fft {
    pub fn new(size: usize, window: Window) -> Self {
        assert!(size > 0);

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(size);

        Self {
            buffer: vec![Default::default(); size],
            scratch: vec![Default::default(); fft.get_inplace_scratch_len()],
            window: window.to_vec(size),
            fft,
            size,
            normalization: 1.0 / (size as f32).sqrt(),
        }
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn forward(&mut self, samples: &[Complex<f32>]) -> &[Complex<f32>] {
        assert_eq!(samples.len(), self.size);

        // apply window
        for i in 0..self.size {
            self.buffer[i] = self.window[i] * samples[i];
        }

        self.fft
            .process_with_scratch(&mut self.buffer, &mut self.scratch);

        // the fft output needs to be normalized with 1/sqrt(n)
        // todo: can't we do the normalization with the input when we apply the window
        // function. is this even worth it? lol
        for x in &mut self.buffer {
            *x *= self.normalization;
        }

        &self.buffer
    }
}

impl Debug for Fft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fft")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

fn hann_window(n: usize) -> Vec<f32> {
    let mut buf = vec![0.0; n + 1];
    let n_f32 = n as f32;
    for i in 0..=n {
        buf[i] = (PI * i as f32 / n_f32).sin().powi(2);
    }
    buf
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    Boxcar,
    Hann,
}

impl FromStr for Window {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "boxcar" => Ok(Self::Boxcar),
            "hann" | "hanning" => Ok(Self::Hann),
            _ => Err(eyre!("No such window: {s}")),
        }
    }
}

impl Window {
    fn to_vec(&self, size: usize) -> Vec<f32> {
        match self {
            Window::Boxcar => vec![1.0; size],
            Window::Hann => hann_window(size - 1),
        }
    }
}
