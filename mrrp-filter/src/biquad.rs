use std::ops::{
    Add,
    Mul,
    Sub,
};

pub use biquad::coefficients::Coefficients;
use biquad::{
    Biquad,
    DirectForm1,
    DirectForm2Transposed,
    Q_BUTTERWORTH_F32,
    ToHertz,
};
use mrrp_core::signal::combinators::Scanner;
use num_traits::{
    ConstZero,
    Zero,
};

/// Wrapper arround [`DirectForm1`] or [`DirectForm2Transposed`].
///
/// This is needed because we can't implement [`Scanner`] on these types
/// directly.
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

pub fn lowpass<T>(
    sample_rate: f32,
    cutoff_frequency: f32,
) -> BiquadScanner<DirectForm2Transposed<f32, T>>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + ConstZero,
    f32: Mul<T, Output = T>,
{
    BiquadScanner(DirectForm2Transposed::new(
        Coefficients::from_params(
            biquad::Type::LowPass,
            sample_rate.hz(),
            cutoff_frequency.hz(),
            Q_BUTTERWORTH_F32,
        )
        .unwrap(),
    ))
}
