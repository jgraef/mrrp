use std::{
    fmt::Debug,
    io::Read,
};

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Invalid(#[from] InvalidFourCC),
}

impl ReadError {
    #[inline]
    pub(super) fn map_invalid(self, f: impl FnOnce([u8; 4]) -> super::Error) -> super::Error {
        match self {
            ReadError::Io(error) => super::Error::Io(error),
            ReadError::Invalid(error) => f(error.bytes),
        }
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid FourCC: {bytes:?}")]
pub struct InvalidFourCC {
    pub bytes: [u8; 4],
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FourCC([u8; 4]);

impl FourCC {
    #[inline]
    pub const fn from_bytes(bytes: [u8; 4]) -> Result<Self, InvalidFourCC> {
        if bytes.is_ascii() {
            Ok(Self(bytes))
        }
        else {
            Err(InvalidFourCC { bytes })
        }
    }

    #[inline]
    pub const fn from_bytes_unchecked(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.0).unwrap()
    }

    #[inline]
    pub fn read<R>(mut reader: R) -> Result<Self, ReadError>
    where
        R: Read,
    {
        let mut buf = [0; 4];
        reader.read_exact(&mut buf)?;
        Ok(Self::from_bytes(buf)?)
    }
}

impl Debug for FourCC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.as_str().fmt(f)
    }
}
