use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

pub use biquad::coefficients::Coefficients;
use biquad::{
    Biquad,
    ToHertz,
};
use num_complex::Complex;
use num_traits::Zero;
use pin_project_lite::pin_project;

use crate::{
    GetSampleRate,
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        ReadBuf,
        ScanInPlaceWith,
        Scanner,
    },
};

pin_project! {
    #[derive(Clone, Debug)]
    pub struct BiquadDf2t<R, S> {
        #[pin]
        input: ScanInPlaceWith<R, BiquadScanner>,
        _phantom: PhantomData<fn(S)>,
    }
}

impl<R, S> BiquadDf2t<R, S>
where
    R: AsyncReadSamples<S>,
{
    pub fn new(input: R, coefficients: Coefficients<f32>) -> Self {
        Self {
            input: input.scan_in_place_with(BiquadScanner {
                biquad: biquad::DirectForm2Transposed::new(coefficients),
            }),
            _phantom: PhantomData,
        }
    }

    pub fn lowpass(input: R, cutoff_frequency: f32) -> Self
    where
        R: GetSampleRate,
    {
        let coefficients = Coefficients::from_params(
            biquad::Type::LowPass,
            input.sample_rate().hz(),
            cutoff_frequency.hz(),
            biquad::Q_BUTTERWORTH_F32,
        )
        .unwrap();
        Self::new(input, coefficients)
    }
}

impl<R> AsyncReadSamples<f32> for BiquadDf2t<R, f32>
where
    R: AsyncReadSamples<f32> + Unpin,
{
    type Error = R::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<f32>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().input.poll_read_samples(cx, buffer)
    }
}

impl<R> AsyncReadSamples<Complex<f32>> for BiquadDf2t<R, Complex<f32>>
where
    R: AsyncReadSamples<Complex<f32>> + Unpin,
{
    type Error = R::Error;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Complex<f32>>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().input.poll_read_samples(cx, buffer)
    }
}

impl<R, S> GetSampleRate for BiquadDf2t<R, S>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.input.sample_rate()
    }
}

#[derive(Clone, Debug)]
struct BiquadScanner {
    biquad: biquad::DirectForm2Transposed<f32>,
}

impl Scanner<f32> for BiquadScanner {
    type Output = f32;

    #[inline]
    fn scan(&mut self, sample: f32) -> Self::Output {
        if sample.is_finite() {
            // non-finite samples seem to mess with the feedback
            self.biquad.run(sample)
        }
        else {
            0.0
        }
    }
}

impl Scanner<Complex<f32>> for BiquadScanner {
    type Output = Complex<f32>;

    #[inline]
    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        if sample.is_finite() {
            // non-finite samples seem to mess with the feedback
            Complex {
                re: self.biquad.run(sample.re),
                im: self.biquad.run(sample.im),
            }
        }
        else {
            Complex::zero()
        }
    }
}
