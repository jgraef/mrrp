use std::ops::{
    Add,
    Mul,
    Sub,
};

pub use biquad;
use biquad::{
    Biquad,
    DirectForm1,
    DirectForm2Transposed,
    Q_BUTTERWORTH_F32,
    ToHertz,
    coefficients::Coefficients,
};
use mrrp_util::signal::Scanner;
use num_traits::{
    ConstZero,
    Zero,
};

/// Wrapper arround [`DirectForm1`] or [`DirectForm2Transposed`].
///
/// This is needed because we can't implement [`Scanner`] on these types
/// directly.
#[derive(Clone, Debug)]
pub struct BiquadScanner<F>(pub F);

impl<C, T> Scanner<T> for BiquadScanner<DirectForm1<C, T>>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + Zero,
    C: Copy + Mul<T, Output = T>,
{
    type Output = T;

    #[inline]
    fn scan(&mut self, sample: T) -> Self::Output {
        self.0.run(sample)
    }
}

impl<C, T> Scanner<T> for BiquadScanner<DirectForm2Transposed<C, T>>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + Zero,
    C: Copy + Mul<T, Output = T>,
{
    type Output = T;

    #[inline]
    fn scan(&mut self, sample: T) -> Self::Output {
        self.0.run(sample)
    }
}

pub fn new<T>(
    sample_rate: f32,
    cutoff_frequency: f32,
    ty: biquad::Type<f32>,
) -> Result<BiquadScanner<DirectForm2Transposed<f32, T>>, Error>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + ConstZero,
    f32: Mul<T, Output = T>,
{
    let coefficients = Coefficients::from_params(
        ty,
        sample_rate.hz(),
        cutoff_frequency.hz(),
        Q_BUTTERWORTH_F32,
    )?;

    Ok(BiquadScanner(DirectForm2Transposed::new(coefficients)))
}

#[inline]
pub fn lowpass<T>(
    sample_rate: f32,
    cutoff_frequency: f32,
) -> Result<BiquadScanner<DirectForm2Transposed<f32, T>>, Error>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + ConstZero,
    f32: Mul<T, Output = T>,
{
    new(sample_rate, cutoff_frequency, biquad::Type::LowPass)
}

#[inline]
pub fn highpass<T>(
    sample_rate: f32,
    cutoff_frequency: f32,
) -> Result<BiquadScanner<DirectForm2Transposed<f32, T>>, Error>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + ConstZero,
    f32: Mul<T, Output = T>,
{
    new(sample_rate, cutoff_frequency, biquad::Type::HighPass)
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum Error {
    #[error("Cutoff frequency outside Nyquist limit for sample rate")]
    OutsideNyquist,
    #[error("Negative Q")]
    NegativeQ,
    #[error("Negative frequency")]
    NegativeFrequency,
}

impl From<biquad::Errors> for Error {
    fn from(value: biquad::Errors) -> Self {
        match value {
            biquad::Errors::OutsideNyquist => Self::OutsideNyquist,
            biquad::Errors::NegativeQ => Self::NegativeQ,
            biquad::Errors::NegativeFrequency => Self::NegativeFrequency,
        }
    }
}
