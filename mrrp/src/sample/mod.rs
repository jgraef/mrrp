mod conversion;
mod types;

use num_complex::Complex;

pub use self::types::{
    I11,
    I20,
    I24,
    I48,
    U11,
    U20,
    U24,
    U48,
};

pub trait Sample: Sized {
    type Signed: FromSample<Self>;
    type Float: FromSample<Self>;
    type Scalar;

    const EQUILIBRIUM: Self;

    #[inline]
    fn into_signed(self) -> Self::Signed {
        self.into_sample()
    }

    #[inline]
    fn into_float(self) -> Self::Float {
        self.into_sample()
    }
}

pub trait FromSample<S> {
    fn from_sample(sample: S) -> Self;
}

impl<T> FromSample<T> for T {
    #[inline]
    fn from_sample(sample: T) -> Self {
        sample
    }
}

pub trait IntoSample<S> {
    fn into_sample(self) -> S;
}

impl<T, U> IntoSample<U> for T
where
    U: FromSample<T>,
{
    #[inline]
    fn into_sample(self) -> U {
        U::from_sample(self)
    }
}

/// A macro used to simplify the implementation of `Sample`.
macro_rules! impl_sample {
    ($($T:ty:
       Signed: $Addition:ty,
       Float: $Modulation:ty,
       EQUILIBRIUM: $EQUILIBRIUM:expr),*) =>
    {
        $(
            impl Sample for $T {
                type Signed = $Addition;
                type Float = $Modulation;
                type Scalar = $T;
                const EQUILIBRIUM: Self = $EQUILIBRIUM;
            }

            impl Sample for Complex<$T> {
                type Signed = Complex<$Addition>;
                type Float = Complex<$Modulation>;
                type Scalar = $T;
                const EQUILIBRIUM: Self = Complex { re: $EQUILIBRIUM, im: $EQUILIBRIUM };
            }
        )*
    }
}

// Expands to `Sample` implementations for all of the following types.
impl_sample! {
    i8:  Signed: i8,  Float: f32, EQUILIBRIUM: 0,
    i16: Signed: i16, Float: f32, EQUILIBRIUM: 0,
    I24: Signed: I24, Float: f32, EQUILIBRIUM: types::i24::EQUILIBRIUM,
    i32: Signed: i32, Float: f32, EQUILIBRIUM: 0,
    I48: Signed: I48, Float: f64, EQUILIBRIUM: types::i48::EQUILIBRIUM,
    i64: Signed: i64, Float: f64, EQUILIBRIUM: 0,
    u8:  Signed: i8,  Float: f32, EQUILIBRIUM: 128,
    u16: Signed: i16, Float: f32, EQUILIBRIUM: 32_768,
    U24: Signed: i32, Float: f32, EQUILIBRIUM: types::u24::EQUILIBRIUM,
    u32: Signed: i32, Float: f32, EQUILIBRIUM: 2_147_483_648,
    U48: Signed: i64, Float: f64, EQUILIBRIUM: types::u48::EQUILIBRIUM,
    u64: Signed: i64, Float: f64, EQUILIBRIUM: 9_223_372_036_854_775_808,
    f32: Signed: f32, Float: f32, EQUILIBRIUM: 0.0,
    f64: Signed: f64, Float: f64, EQUILIBRIUM: 0.0
}
