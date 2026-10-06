use num_complex::Complex;

#[derive(Clone, Copy, Debug)]
pub enum BigEndian {}

#[derive(Clone, Copy, Debug)]
pub enum LittleEndian {}

pub trait Endianess: sealed::Sealed {}

impl Endianess for BigEndian {}
impl sealed::Sealed for BigEndian {}

impl Endianess for LittleEndian {}
impl sealed::Sealed for LittleEndian {}

pub type NetworkEndian = BigEndian;

#[cfg(target_endian = "big")]
pub type NativeEndian = BigEndian;

#[cfg(target_endian = "little")]
pub type NativeEndian = LittleEndian;

mod sealed {
    pub trait Sealed {}
}

pub trait Bytes: Sized {
    const LEN: usize = 1;

    fn from_slice(slice: &[u8]) -> Self;
    fn as_slice(&self) -> &[u8];
}

impl<const N: usize> Bytes for [u8; N] {
    const LEN: usize = N;

    #[inline]
    fn from_slice(slice: &[u8]) -> Self {
        slice.try_into().unwrap()
    }

    #[inline]
    fn as_slice(&self) -> &[u8] {
        self
    }
}

pub trait Encoding {
    type Encoded: Bytes;
}

pub trait Encode<E>: Encoding
where
    E: Endianess,
{
    type Error;

    fn encode(&self) -> Result<Self::Encoded, Self::Error>;
}

pub trait Decode<E>: Encoding + Sized
where
    E: Endianess,
{
    type Error;

    fn decode(bytes: Self::Encoded) -> Result<Self, Self::Error>;
}

macro_rules! impl_int {
    ($ty:ty, $bytes:expr) => {
        impl Encoding for $ty {
            type Encoded = [u8; $bytes];
        }

        impl Encoding for Complex<$ty> {
            type Encoded = [u8; $bytes * 2];
        }

        impl_int!(@for($ty, $bytes, BigEndian, from_be_bytes, to_be_bytes));
        impl_int!(@for($ty, $bytes, LittleEndian, from_le_bytes, to_le_bytes));
    };
    (@for($ty:ty, $bytes:expr, $endianess:ident, $from_bytes:ident, $to_bytes:ident)) => {
        impl Encode<$endianess> for $ty {
            type Error = !;

            #[inline]
            fn encode(&self) -> Result<Self::Encoded, Self::Error> {
                Ok(self.$to_bytes())
            }
        }

        impl Decode<$endianess> for $ty {
            type Error = !;

            #[inline]
            fn decode(bytes: Self::Encoded) -> Result<Self, Self::Error> {
                Ok(Self::$from_bytes(bytes))
            }
        }

        impl Encode<$endianess> for Complex<$ty> {
            type Error = !;

            #[inline]
            fn encode(&self) -> Result<Self::Encoded, Self::Error> {
                let mut buf = [0; $bytes * 2];
                buf[..$bytes].copy_from_slice(&self.re.$to_bytes());
                buf[$bytes..].copy_from_slice(&self.im.$to_bytes());
                Ok(buf)
            }
        }

        impl Decode<$endianess> for Complex<$ty> {
            type Error = !;

            #[inline]
            fn decode(bytes: Self::Encoded) -> Result<Self, Self::Error> {
                let re = <$ty>::$from_bytes(bytes[..$bytes].try_into().unwrap());
                let im = <$ty>::$from_bytes(bytes[$bytes..].try_into().unwrap());
                Ok(Complex { re, im })
            }
        }
    };
}

impl_int!(u8, 1);
impl_int!(i8, 1);
impl_int!(u16, 2);
impl_int!(u32, 4);
impl_int!(u64, 8);
impl_int!(i16, 2);
impl_int!(i32, 4);
impl_int!(i64, 8);
impl_int!(f32, 4);
impl_int!(f64, 8);
