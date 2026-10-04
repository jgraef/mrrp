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

impl Encoding for u8 {
    type Encoded = [u8; 1];
}

impl<E> Encode<E> for u8
where
    E: Endianess,
{
    type Error = !;

    #[inline]
    fn encode(&self) -> Result<Self::Encoded, Self::Error> {
        Ok([*self])
    }
}

impl<E> Decode<E> for u8
where
    E: Endianess,
{
    type Error = !;

    #[inline]
    fn decode(bytes: Self::Encoded) -> Result<Self, Self::Error> {
        Ok(bytes[0])
    }
}

impl Encoding for i8 {
    type Encoded = [u8; 1];
}

impl<E> Encode<E> for i8
where
    E: Endianess,
{
    type Error = !;

    #[inline]
    fn encode(&self) -> Result<Self::Encoded, Self::Error> {
        <u8 as Encode<E>>::encode(&self.cast_unsigned())
    }
}

impl<E> Decode<E> for i8
where
    E: Endianess,
{
    type Error = !;

    #[inline]
    fn decode(bytes: Self::Encoded) -> Result<Self, Self::Error> {
        Ok(<u8 as Decode<E>>::decode(bytes)?.cast_signed())
    }
}

macro_rules! impl_unsigned {
    ($unsigned:ty) => {
        impl Encode<BigEndian> for $unsigned {
            type Error = !;

            #[inline]
            fn encode(&self) -> Result<Self::Encoded, Self::Error> {
                Ok(self.to_be_bytes())
            }
        }

        impl Decode<BigEndian> for $unsigned {
            type Error = !;

            #[inline]
            fn decode(bytes: Self::Encoded) -> Result<Self, Self::Error> {
                Ok(Self::from_be_bytes(bytes))
            }
        }

        impl Encode<LittleEndian> for $unsigned {
            type Error = !;

            #[inline]
            fn encode(&self) -> Result<Self::Encoded, Self::Error> {
                Ok(self.to_le_bytes())
            }
        }

        impl Decode<LittleEndian> for $unsigned {
            type Error = !;

            #[inline]
            fn decode(bytes: Self::Encoded) -> Result<Self, Self::Error> {
                Ok(Self::from_le_bytes(bytes))
            }
        }
    };
}

macro_rules! impl_signed {
    ($signed:ty, $unsigned:ty, $endianess:ty) => {
        impl Encode<$endianess> for $signed {
            type Error = !;

            #[inline]
            fn encode(&self) -> Result<Self::Encoded, Self::Error> {
                <$unsigned as Encode<$endianess>>::encode(&self.cast_unsigned())
            }
        }

        impl Decode<$endianess> for $signed {
            type Error = !;

            #[inline]
            fn decode(bytes: Self::Encoded) -> Result<Self, Self::Error> {
                Ok(<$unsigned as Decode<$endianess>>::decode(bytes)?.cast_signed())
            }
        }
    };
}

macro_rules! impl_encoding {
    ($signed:ty, $unsigned:ty, $bytes:expr) => {
        impl Encoding for $signed {
            type Encoded = [u8; $bytes];
        }

        impl Encoding for $unsigned {
            type Encoded = [u8; $bytes];
        }

        impl_unsigned!($unsigned);
        impl_signed!($signed, $unsigned, BigEndian);
        impl_signed!($signed, $unsigned, LittleEndian);
    };
}

impl_encoding!(i16, u16, 2);
impl_encoding!(i32, u32, 4);
impl_encoding!(i64, u64, 8);
