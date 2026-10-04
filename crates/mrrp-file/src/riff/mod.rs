mod four_cc;

use std::{
    fmt::Debug,
    io::{
        Read,
        Seek,
        SeekFrom,
    },
};

use byteorder::{
    LittleEndian,
    ReadBytesExt,
};

use crate::riff::four_cc::FourCC;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("Expected RIFF chunk, but got {id:?}")]
    InvalidHeader { id: ChunkId },

    #[error("Invalid chunk ID: {bytes:?}")]
    InvalidChunkId { bytes: [u8; 4] },
}

#[derive(Clone, Copy, Debug)]
pub struct Reader<R> {
    reader: R,
}

impl<R> Reader<R> {
    pub fn new(reader: R) -> Self {
        Self { reader }
    }
}

impl<R> Reader<R>
where
    R: Read + Seek,
{
    pub fn riff(&mut self) -> Result<ChunkRef, Error> {
        self.reader.seek(SeekFrom::Start(0))?;
        let chunk_ref = self.read_chunk_ref()?;

        if chunk_ref.id == ChunkId::RIFF {
            Ok(chunk_ref)
        }
        else {
            Err(Error::InvalidHeader { id: chunk_ref.id })
        }
    }

    fn read_chunk_ref(&mut self) -> Result<ChunkRef, Error> {
        let id = ChunkId::read(&mut self.reader)?;
        let size = self.reader.read_u32::<LittleEndian>()?;
        let offset = self.reader.stream_position()?;

        Ok(ChunkRef { id, size, offset })
    }

    pub fn list<'a>(&'a mut self, chunk_ref: ChunkRef) -> Result<ListReader<'a, R>, Error> {
        self.reader.seek(SeekFrom::Start(chunk_ref.offset))?;

        let list_id = ListId::read(&mut self.reader)?;
        tracing::debug!(?chunk_ref, ?list_id, "list");

        Ok(ListReader {
            reader: Reader::new(LimitReader::new(
                &mut self.reader,
                chunk_ref.offset,
                chunk_ref.size.into(),
            )),
            skip: 0,
            list_id,
        })
    }

    pub fn data<'a>(&'a mut self, chunk_ref: ChunkRef) -> Result<DataReader<'a, R>, Error> {
        self.reader.seek(SeekFrom::Start(chunk_ref.offset))?;
        Ok(DataReader {
            inner: LimitReader::new(&mut self.reader, chunk_ref.offset, chunk_ref.size.into()),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ChunkRef {
    /// Chunk ID
    id: ChunkId,

    /// Size of the chunk payload
    size: u32,

    /// Offset of the chunk payload
    offset: u64,
}

impl ChunkRef {
    #[inline]
    pub fn id(&self) -> ChunkId {
        self.id
    }

    #[inline]
    pub fn size(&self) -> u32 {
        self.size
    }

    #[inline]
    pub fn offset(&self) -> u64 {
        self.offset
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChunkId(FourCC);

impl ChunkId {
    pub const RIFF: Self = Self::from_bytes_unchecked(*b"RIFF");
    pub const LIST: Self = Self::from_bytes_unchecked(*b"LIST");
    pub const INFO: Self = Self::from_bytes_unchecked(*b"INFO");
    pub const JUNK: Self = Self::from_bytes_unchecked(*b"JUNK");

    #[inline]
    pub const fn from_bytes(bytes: [u8; 4]) -> Result<Self, Error> {
        match FourCC::from_bytes(bytes) {
            Ok(four_cc) => Ok(Self(four_cc)),
            Err(error) => Err(Error::InvalidChunkId { bytes: error.bytes }),
        }
    }

    #[inline]
    pub const fn from_bytes_unchecked(bytes: [u8; 4]) -> Self {
        Self(FourCC::from_bytes_unchecked(bytes))
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn read<R>(reader: R) -> Result<Self, Error>
    where
        R: Read,
    {
        FourCC::read(reader)
            .map(Self)
            .map_err(|error| error.map_invalid(|bytes| Error::InvalidChunkId { bytes }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ListId(FourCC);

impl ListId {
    pub const WAVE: Self = Self::from_bytes_unchecked(*b"WAVE");

    #[inline]
    pub const fn from_bytes(bytes: [u8; 4]) -> Result<Self, Error> {
        match FourCC::from_bytes(bytes) {
            Ok(four_cc) => Ok(Self(four_cc)),
            Err(error) => Err(Error::InvalidChunkId { bytes: error.bytes }),
        }
    }

    #[inline]
    pub const fn from_bytes_unchecked(bytes: [u8; 4]) -> Self {
        Self(FourCC::from_bytes_unchecked(bytes))
    }

    #[inline]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub fn read<R>(reader: R) -> Result<Self, Error>
    where
        R: Read,
    {
        FourCC::read(reader)
            .map(Self)
            .map_err(|error| error.map_invalid(|bytes| Error::InvalidChunkId { bytes }))
    }
}

#[derive(Debug)]
pub struct ListReader<'a, R> {
    reader: Reader<LimitReader<&'a mut R>>,
    skip: u32,
    list_id: ListId,
}

impl<'a, R> ListReader<'a, R>
where
    R: Read + Seek,
{
    fn read_inner(&mut self) -> Result<ChunkRef, Error> {
        if self.skip > 0 {
            // first skip the contents of the previous chunk

            self.reader
                .reader
                .seek(SeekFrom::Current(self.skip.into()))?;
        }

        let chunk_ref = self.reader.read_chunk_ref()?;

        // remember that we need to skip this to reach the next chunk
        self.skip = chunk_ref.size;

        Ok(chunk_ref)
    }

    pub fn next(&mut self) -> Result<Option<ChunkRef>, Error> {
        match self.read_inner() {
            Ok(chunk) => Ok(Some(chunk)),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn list_id(&self) -> ListId {
        self.list_id
    }
}

#[derive(Debug)]
pub struct DataReader<'a, R> {
    inner: LimitReader<&'a mut R>,
}

impl<'a, R> Read for DataReader<'a, R>
where
    R: Read + Seek,
{
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        self.inner.read(buf)
    }
}

impl<'a, R> Seek for DataReader<'a, R>
where
    R: Seek,
{
    #[inline]
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(pos)
    }
}

#[derive(Clone, Copy, Debug)]
struct LimitReader<R> {
    inner: R,
    offset: u64,
    size: u64,
}

impl<R> LimitReader<R> {
    pub fn new(inner: R, offset: u64, size: u64) -> Self {
        Self {
            inner,
            offset,
            size,
        }
    }
}

impl<R> Read for LimitReader<R>
where
    R: Read + Seek,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, std::io::Error> {
        let position = self.inner.stream_position()?;
        let remaining = (self.size + self.offset).saturating_sub(position);

        let n_limited = buf.len().min(remaining.try_into().unwrap_or(usize::MAX));

        self.inner.read(&mut buf[..n_limited])
    }
}

impl<R> Seek for LimitReader<R>
where
    R: Seek,
{
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let offset = match pos {
            SeekFrom::Start(offset) => Some(self.offset + offset),
            SeekFrom::End(offset) => (self.offset + self.size).checked_add_signed(offset),
            SeekFrom::Current(offset) => self.inner.stream_position()?.checked_add_signed(offset),
        };

        if let Some(offset) = offset
            && offset >= self.offset
            && offset < self.offset + self.size
        {
            self.inner.seek(SeekFrom::Start(offset))
        }
        else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }
}
