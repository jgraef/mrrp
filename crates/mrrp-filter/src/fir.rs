use std::{
    collections::VecDeque,
    ops::{
        Add,
        Mul,
    },
};

use mrrp_util::signal::{
    ScanInPlaceWith,
    Scanner,
};
use num_traits::{
    Float,
    FloatConst,
    FromPrimitive,
};

#[derive(Clone, Debug)]
pub struct FirFilter<S, C> {
    coefficients: Vec<C>,
    delayed: VecDeque<S>,
}

impl<S, C> FirFilter<S, C> {
    #[inline]
    pub fn new(coefficients: Vec<C>) -> Self {
        assert!(coefficients.len() > 1);

        let delayed = VecDeque::with_capacity(coefficients.len() - 1);

        Self {
            coefficients,
            delayed,
        }
    }
}

impl<S, C> Scanner<S> for FirFilter<S, C>
where
    S: Copy + Mul<C, Output = S> + Add<S, Output = S>,
    C: Copy,
{
    type Output = S;

    fn scan(&mut self, sample: S) -> Self::Output {
        debug_assert!(self.delayed.len() < self.coefficients.len());

        let mut output = sample * self.coefficients[0];
        for (delayed, coeff) in self.delayed.iter().zip(&self.coefficients[1..]) {
            output = output + *delayed * *coeff;
        }

        if self.delayed.len() == self.coefficients.len() - 1 {
            self.delayed.pop_back();
        }
        self.delayed.push_front(sample);

        output
    }
}

pub type FirFiltered<R, S, C> = ScanInPlaceWith<R, FirFilter<S, C>>;

pub fn hann_window<T>(n: usize) -> impl Iterator<Item = T>
where
    T: Float + FloatConst + FromPrimitive,
{
    let n_t = T::from_usize(n).unwrap();
    (0..=n).map(move |i| (T::PI() * T::from_usize(i).unwrap() / n_t).sin().powi(2))
}

#[cfg(test)]
mod tests {
    use mrrp_util::signal::{
        AsyncReadSamplesExt,
        Cursor,
        white_noise,
    };
    use rand::rngs::SmallRng;

    use crate::fir::{
        FirFilter,
        hann_window,
    };

    fn convolve(x: &[f32], h: &[f32]) -> Vec<f32> {
        let mut y = vec![0.0; x.len()];
        for i in 0..x.len() {
            for j in 0..h.len() {
                if i >= j
                    && let Some(x) = x.get(i - j)
                {
                    y[i] += x * h[j];
                }
            }
        }
        y
    }

    #[tokio::test]
    async fn test_fir_filter_against_reference_convolution() {
        let mut x = vec![];
        white_noise::<SmallRng, f32>(rand::make_rng())
            .limit(20)
            .read_to_end(&mut x)
            .await
            .unwrap();

        let h = hann_window(5).collect::<Vec<f32>>();

        let expected = convolve(&x, &h);

        let mut y = vec![];
        Cursor::new(&x[..])
            .scan_in_place_with(FirFilter::new(h))
            .read_to_end(&mut y)
            .await
            .unwrap();

        assert_eq!(expected, y);
    }
}
