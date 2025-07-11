use std::{
    fmt::Debug,
    sync::Arc,
};

use num_complex::Complex;
use rustfft::FftPlanner;

pub struct Fft {
    buffer: Vec<Complex<f32>>,
    scratch: Vec<Complex<f32>>,
    fft: Arc<dyn rustfft::Fft<f32>>,
    size: usize,
    normalization: f32,
}

impl Fft {
    pub fn new(size: usize) -> Self {
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(size);

        Self {
            buffer: vec![Default::default(); size],
            scratch: vec![Default::default(); fft.get_immutable_scratch_len()],
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
        self.fft
            .process_immutable_with_scratch(samples, &mut self.buffer, &mut self.scratch);

        // the fft output needs to be normalized with 1/sqrt(n)
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
