use std::fmt::Debug;

use bitflags::bitflags;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkHeader {
    pub tag: Tag,
    pub flags: Flags,
    pub size: u64,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tag([u8; 4]);

impl Tag {
    pub const MIQH: Self = Self::from_bytes_unchecked(*b"MIQH");

    #[inline]
    pub const fn from_bytes(bytes: [u8; 4]) -> Result<Self, InvalidTag> {
        if bytes.is_ascii() {
            Ok(Self(bytes))
        }
        else {
            Err(InvalidTag { bytes })
        }
    }

    #[inline]
    pub const fn from_bytes_unchecked(bytes: [u8; 4]) -> Self {
        Self(bytes)
    }

    #[inline]
    pub const fn to_bytes(&self) -> [u8; 4] {
        self.0
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        str::from_utf8(&self.0).unwrap()
    }
}

impl Debug for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Tag").field(&self.as_str()).finish()
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid MIQ tag: {bytes:?}")]
pub struct InvalidTag {
    pub bytes: [u8; 4],
}

#[derive(Debug, thiserror::Error)]
pub enum TagReadError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Invalid(#[from] InvalidTag),
}

bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
    pub struct Flags: u32 {
        /// The chunk contains nested chunks.
        ///
        /// This indicates to a reader that the data in this chunk is a sequence of chunks.
        ///
        /// At the time of writing this feature is not implemented, and use of this feature is discouraged.
        ///
        /// TODO: Go more into detail how this can work, and why it might not be a good idea.
        const DIRECTORY = 0x00_00_00_01;

        /// The chunks should be ignored.
        ///
        /// This can be used to delete chunks by just setting a single bit. The chunk will still be physically there, but should be ignored by readers.
        const IGNORE = 0x00_00_00_02;

        /// The chunk was patched.
        ///
        /// A patched chunk's contents should be ignored.
        /// The first 8 bytes of the chunk's content have been overwritten with the offset to the replacement chunk.
        /// This offset points to the start of a chunk header.
        /// The replacement chunk must have the same tag as the patched chunk.
        ///
        /// This flag must not be used during streaming.
        const PATCHED = 0x00_00_00_04;

        /// The chunk contains structured data encoded with CBOR
        ///
        /// This is just a hint and can be ignored. How a chunk is interpreted is defined by its tag alone.
        /// Nevertheless for inspection and debugging by software, that might not know the meaning of a specific tag value,
        /// it can be useful to know that a chunk contains CBOR.
        const CBOR = 0x00_00_00_08;
    }
}

impl Flags {
    #[inline]
    pub const fn from_bytes(bytes: [u8; 4]) -> Self {
        Self::from_bits_retain(u32::from_be_bytes(bytes))
    }

    #[inline]
    pub const fn to_bytes(&self) -> [u8; 4] {
        self.bits().to_be_bytes()
    }
}

pub trait Tagged {
    const TAG: Tag;
}
