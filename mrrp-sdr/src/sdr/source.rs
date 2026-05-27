use std::task::{
    Context,
    Poll,
};

use anyhow::Error;
use num_complex::Complex;

// todo: use mrrp-core trait
pub trait AsyncReadSamples {
    fn poll_read_samples(
        &mut self,
        cx: &mut Context,
        buffer: &mut [Complex<f32>],
    ) -> Poll<Result<(), Error>>;
}
