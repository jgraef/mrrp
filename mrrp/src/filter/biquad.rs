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
use num_traits::Zero;

use crate::io::combinators::Scanner;

impl<C, T> Scanner<T> for DirectForm1<C, T>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + Zero,
    C: Copy + Mul<T, Output = T>,
{
    type Output = T;

    #[inline]
    fn scan(&mut self, sample: T) -> Self::Output {
        self.run(sample)
    }
}

impl<C, T> Scanner<T> for DirectForm2Transposed<C, T>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + Zero,
    C: Copy + Mul<T, Output = T>,
{
    type Output = T;

    #[inline]
    fn scan(&mut self, sample: T) -> Self::Output {
        self.run(sample)
    }
}

pub fn lowpass<T>(sample_rate: f32, cutoff_frequency: f32) -> DirectForm2Transposed<f32, T>
where
    T: Copy + Add<T, Output = T> + Sub<T, Output = T> + Zero,
    f32: Mul<T, Output = T>,
{
    DirectForm2Transposed::new(
        Coefficients::from_params(
            biquad::Type::LowPass,
            sample_rate.hz(),
            cutoff_frequency.hz(),
            Q_BUTTERWORTH_F32,
        )
        .unwrap(),
    )
}
