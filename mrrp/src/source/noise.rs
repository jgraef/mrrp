use std::{
    convert::Infallible,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use num_complex::{
    Complex,
    ComplexDistribution,
};
use pin_project_lite::pin_project;
use rand::{
    Rng,
    SeedableRng,
    distributions::{
        Distribution,
        Uniform,
    },
    rngs::SmallRng,
    thread_rng,
};

use crate::io::{
    AsyncReadSamples,
    ReadBuf,
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

pub fn white_noise<S: WhiteNoise>() -> Noise<SmallRng, S::Distribution> {
    Noise::new(default_rng(), S::distribution())
}

fn default_rng() -> SmallRng {
    SmallRng::from_rng(thread_rng()).unwrap()
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
                Uniform::new_inclusive($min, $max)
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
    ComplexDistribution<<T as WhiteNoise>::Distribution>: Distribution<Complex<T>>,
{
    type Distribution = ComplexDistribution<<T as WhiteNoise>::Distribution>;

    fn distribution() -> Self::Distribution {
        ComplexDistribution::new(T::distribution(), T::distribution())
    }
}
