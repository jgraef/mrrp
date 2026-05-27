use std::{
    convert::Infallible,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use num_complex::Complex;
use pin_project_lite::pin_project;
use rand::{
    Rng,
    RngExt,
    distr::{
        Distribution,
        Uniform,
    },
};

use crate::io::{
    AsyncReadSamples,
    ReadBuf,
    Remaining,
    StreamLength,
};

pin_project! {
    #[derive(Clone, Debug, Default)]
    pub struct Noise<R, D> {
        rng: R,
        distribution: D,
    }
}

impl<R, D> Noise<R, D> {
    #[inline]
    pub fn new(rng: R, distribution: D) -> Self {
        Self { rng, distribution }
    }
}

impl<R, D, S> AsyncReadSamples<S> for Noise<R, D>
where
    R: Rng,
    D: Distribution<S>,
{
    type Error = Infallible;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        buffer.fill_with(|| this.rng.sample(&*this.distribution));
        Poll::Ready(Ok(()))
    }
}

impl<R, D> StreamLength for Noise<R, D> {
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Infinite
    }
}

pub fn white_noise<R, S>(rng: R) -> Noise<R, S::Distribution>
where
    R: Rng,
    S: WhiteNoise,
{
    Noise::new(rng, S::distribution())
}

pub trait WhiteNoise: Sized {
    type Distribution: Distribution<Self>;

    fn distribution() -> Self::Distribution;
}

macro_rules! impl_white_noise {
    ($T:ty) => {
        impl_white_noise!($T: <$T>::MIN, <$T>::MAX);
    };
    ($T:ty: $min:expr, $max:expr) => {
        impl WhiteNoise for $T {
            type Distribution = Uniform<$T>;

            fn distribution() -> Self::Distribution {
                Uniform::new_inclusive($min, $max).expect(concat!("Could not create uniform random distribution for {}", stringify!($T)))
            }
        }
    };
}

impl_white_noise!(u8);
impl_white_noise!(i8);
impl_white_noise!(u16);
impl_white_noise!(i16);
impl_white_noise!(u32);
impl_white_noise!(i32);
impl_white_noise!(u64);
impl_white_noise!(i64);
impl_white_noise!(f32: -1.0, 1.0);
impl_white_noise!(f64: -1.0, 1.0);

impl<T> WhiteNoise for Complex<T>
where
    T: WhiteNoise,
{
    type Distribution =
        ComplexDistribution<<T as WhiteNoise>::Distribution, <T as WhiteNoise>::Distribution>;

    fn distribution() -> Self::Distribution {
        ComplexDistribution {
            re: T::distribution(),
            im: T::distribution(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ComplexDistribution<Re, Im> {
    pub re: Re,
    pub im: Im,
}

impl<T, Re, Im> Distribution<Complex<T>> for ComplexDistribution<Re, Im>
where
    Re: Distribution<T>,
    Im: Distribution<T>,
{
    fn sample<R: Rng + ?Sized>(&self, mut rng: &mut R) -> Complex<T> {
        let re = self.re.sample(&mut rng);
        let im = self.im.sample(&mut rng);
        Complex { re, im }
    }
}
