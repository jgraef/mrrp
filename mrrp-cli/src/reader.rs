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
    chunk: Option<Chunk<Iq>>,
    read_pos: usize,
    buffer: Vec<Complex<f32>>,
    write_pos: usize,
}

impl SampleReader {
    pub fn new(samples: Samples<Iq>) -> Self {
        Self {
            samples,
            chunk: None,
            read_pos: 0,
            buffer: vec![],
            write_pos: 0,
        }
    }

    pub async fn read(&mut self, num_samples: usize) -> Result<Option<&'_ [Complex<f32>]>, Error> {
        self.buffer.resize(num_samples, Default::default());

        while self.write_pos < num_samples {
            if let Some(chunk) = &self.chunk {
                let samples = chunk.samples();
                while self.write_pos < num_samples && self.read_pos < samples.len() {
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
