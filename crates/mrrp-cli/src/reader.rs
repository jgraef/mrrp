use futures_util::TryStreamExt;
use num_complex::Complex;
use rtlsdr_async::{
    Chunk,
    Iq,
    Samples,
};

use crate::Error;

#[derive(Debug)]
pub struct SampleReader {
    samples: Samples<Iq>,
    segment_size: usize,
    overlap: usize,
    chunk: Option<Chunk<Iq>>,
    read_pos: usize,
    buffer: Vec<Complex<f32>>,
    write_pos: usize,
    first_segment: bool,
}

impl SampleReader {
    pub fn new(samples: Samples<Iq>, segment_size: usize, overlap: usize) -> Self {
        assert!(overlap < segment_size);
        Self {
            samples,
            segment_size,
            overlap,
            chunk: None,
            read_pos: 0,
            buffer: vec![Default::default(); segment_size],
            write_pos: 0,
            first_segment: true,
        }
    }

    pub async fn read(&mut self) -> Result<Option<&'_ [Complex<f32>]>, Error> {
        if !self.first_segment && self.write_pos == 0 && self.overlap != 0 {
            self.buffer
                .copy_within(self.segment_size - self.overlap.., 0);
            self.write_pos = self.overlap
        }

        while self.write_pos < self.segment_size {
            if let Some(chunk) = &self.chunk {
                let samples = chunk.samples();
                while self.write_pos < self.segment_size && self.read_pos < samples.len() {
                    self.buffer[self.write_pos] = samples[self.read_pos].into();
                    self.write_pos += 1;
                    self.read_pos += 1;
                }

                if self.read_pos >= samples.len() {
                    self.chunk = None;
                    self.read_pos = 0;
                }
            }
            else {
                self.chunk = self.samples.try_next().await?;
                if self.chunk.is_none() {
                    return Ok(None);
                }
            }
        }

        self.write_pos = 0;

        Ok(Some(&self.buffer))
    }
}
