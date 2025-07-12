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
        // todo: i need clarification about what this does exactly. as far as I can see
        // this is undone in later normalization anyway.

        // https://stackoverflow.com/questions/20165193/fft-normalization
        // we will normalize later
        //for x in &mut self.buffer {
        //    *x *= self.normalization;
        //}

        // center frequency will be in bin 0. right of center upto n/2 - 1. rest is left
        // of center, so we need to swap halves.
        // of course we could combine this with the earlier normalization loop, but for
        // clarity we'll keep it separate for now.
        let upper_half_offset = self.size.div_ceil(2);
        for i in 0..(self.size / 2) {
            let j = upper_half_offset + i;
            let tmp = self.buffer[i];
            self.buffer[i] = self.buffer[j];
            self.buffer[j] = tmp;
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

fn hann_window(n: usize) -> impl Iterator<Item = f32> {
    let n_f32 = n as f32;
    (0..=n).map(move |i| (PI * i as f32 / n_f32).sin().powi(2))
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
            Window::Boxcar => std::iter::repeat_n(1.0, size).collect(),
            Window::Hann => hann_window(size - 1).collect(),
        }
    }
}
