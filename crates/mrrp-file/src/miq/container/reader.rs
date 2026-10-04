use std::{
    fs::File,
    io::{
        BufRead,
        BufReader,
        Read,
        Seek,
        Take,
    },
    path::Path,
};

use serde::de::DeserializeOwned;

use crate::miq::container::{
    chunk::{
        ChunkHeader,
        Flags,
        InvalidTag,
        Tag,
    },
    header::FileHeader,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    InvalidTag(#[from] InvalidTag),

    #[error("Invalid MIQ file header: {bytes:?}")]
    InvalidHeader { bytes: [u8; 4] },

    #[error(transparent)]
    Cbor(#[from] ciborium::de::Error<std::io::Error>),
}

#[derive(Debug)]
pub struct Reader<R> {
    reader: R,

    /// Scratch buffer used bu ciborium.
    ///
    /// [`ciborium::from_reader`] uses a stack-allocated 4K buffer. We can avoid
    /// this by allocating a buffer on the heap once. We can also use a much
    /// larger buffer size, if needed.
    ///
    /// # TODO
    ///
    /// - Make buffer size customizable.
    /// - For embedded this will need to be stack-allocated.
    scratch_buffer: Vec<u8>,

    /// How many bytes need to be skipped to reach the next chunk header.
    next_chunk_offset: u64,
}

impl<R> Reader<R> {
    #[inline]
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            scratch_buffer: vec![0; 0x1000],
            next_chunk_offset: 0,
        }
    }
}

impl Reader<BufReader<File>> {
    #[inline]
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(Self::new(BufReader::new(File::open(path)?)))
    }
}

impl<R> Reader<R>
where
    R: Read,
{
    pub fn read_chunk(&mut self) -> Result<ReadChunk<'_, R>, Error> {
        // skip to next chunk
        skip(
            &mut self.reader,
            self.next_chunk_offset,
            &mut self.scratch_buffer,
        )?;
        self.next_chunk_offset = 0;

        // read chunk header
        let chunk_header = read_chunk_header(&mut self.reader)?;

        Ok(ReadChunk::new(
            &mut self.reader,
            chunk_header,
            &mut self.scratch_buffer,
            &mut self.next_chunk_offset,
        ))
    }

    /// Try to read a chunk, or return `None` if EOF.
    ///
    /// # TODO
    ///
    /// Choose a better name
    #[inline]
    pub fn try_read_chunk(&mut self) -> Result<Option<ReadChunk<'_, R>>, Error> {
        match self.read_chunk() {
            Ok(chunk) => Ok(Some(chunk)),
            Err(Error::Io(error)) if error.kind() == std::io::ErrorKind::UnexpectedEof => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn read_file_header(&mut self) -> Result<FileHeader, Error> {
        let mut chunk = self.read_chunk().map_err(|error| {
            // if we read an invalid tag in the header, it's just not a MIQ
            // file.
            match error {
                Error::InvalidTag(InvalidTag { bytes }) => Error::InvalidHeader { bytes },
                error => error,
            }
        })?;

        // check header tag
        let chunk_header = chunk.chunk_header();
        if chunk_header.tag != Tag::MIQH {
            return Err(Error::InvalidHeader {
                bytes: chunk_header.tag.to_bytes(),
            });
        }

        Ok(chunk.read_cbor::<FileHeader>()?)
    }
}

/// Low-level wrapper that only reads the chunk's contents.
///
/// This takes care of limiting the reader and tracking the offset to the next
/// chunk header.
///
/// [`ReadChunk`] wraps this but adds functionality for deserializing CBOR.
#[derive(Debug)]
struct ChunkContentReader<'a, R> {
    /// Reader limited to the extent of the chunk's contents
    reader: Take<&'a mut R>,

    /// How many bytes need to be skipped to reach the next chunk header. This
    /// needs to be updated by methods on this type.
    next_chunk_offset: &'a mut u64,

    /// This is needed for seek implementation
    padding_length: u64,
}

impl<'a, R> ChunkContentReader<'a, R>
where
    R: Read,
{
    #[inline]
    fn new(reader: &'a mut R, chunk_size: u64, next_chunk_offset: &'a mut u64) -> Self {
        // calculate the offset to the next chunk header.
        *next_chunk_offset = chunk_size.max(8).next_multiple_of(8);
        let padding_length = *next_chunk_offset - chunk_size;

        Self {
            reader: reader.take(chunk_size),
            next_chunk_offset,
            padding_length,
        }
    }
}

impl<'a, R> Read for ChunkContentReader<'a, R>
where
    R: Read,
{
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n_read = self.reader.read(buf)?;
        *self.next_chunk_offset -= u64::try_from(n_read).unwrap();
        Ok(n_read)
    }
}

impl<'a, R> Seek for ChunkContentReader<'a, R>
where
    R: Seek,
{
    // todo: write a test for this
    #[inline]
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let new_pos = self.reader.seek(pos)?;

        // we need to recalculate the remaining offset to the next chunk header
        *self.next_chunk_offset = self.reader.limit() + self.padding_length;

        Ok(new_pos)
    }
}

impl<'a, R> BufRead for ChunkContentReader<'a, R>
where
    R: BufRead,
{
    #[inline]
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.reader.fill_buf()
    }

    #[inline]
    fn consume(&mut self, amount: usize) {
        self.reader.consume(amount);
        *self.next_chunk_offset -= u64::try_from(amount).unwrap();
    }
}

#[derive(Debug)]
pub struct ReadChunk<'a, R> {
    content_reader: ChunkContentReader<'a, R>,

    /// The [`ChunkHeader`] for this chunk
    chunk_header: ChunkHeader,

    /// Scratch buffer used bu ciborium
    scratch_buffer: &'a mut [u8],
}

impl<'a, R> ReadChunk<'a, R> {
    #[inline]
    pub fn chunk_header(&self) -> &ChunkHeader {
        &self.chunk_header
    }
}

impl<'a, R> ReadChunk<'a, R>
where
    R: Read,
{
    #[inline]
    fn new(
        reader: &'a mut R,
        chunk_header: ChunkHeader,
        scratch_buffer: &'a mut [u8],
        next_chunk_offset: &'a mut u64,
    ) -> Self {
        Self {
            content_reader: ChunkContentReader::new(reader, chunk_header.size, next_chunk_offset),
            chunk_header,
            scratch_buffer: scratch_buffer,
        }
    }

    #[inline]
    pub fn read_cbor<T>(&mut self) -> Result<T, Error>
    where
        T: DeserializeOwned,
    {
        Ok(ciborium::from_reader_with_buffer(
            &mut self.content_reader,
            &mut self.scratch_buffer,
        )?)
    }
}

impl<'a, R> Read for ReadChunk<'a, R>
where
    R: Read,
{
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.content_reader.read(buf)
    }
}

impl<'a, R> Seek for ReadChunk<'a, R>
where
    R: Seek,
{
    // todo: write a test for this
    #[inline]
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.content_reader.seek(pos)
    }
}

impl<'a, R> BufRead for ReadChunk<'a, R>
where
    R: BufRead,
{
    #[inline]
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        self.content_reader.fill_buf()
    }

    #[inline]
    fn consume(&mut self, amount: usize) {
        self.content_reader.consume(amount);
    }
}

/// Low-level function to read a [`ChunkHeader`] from a [`Read`].
pub fn read_chunk_header<R>(mut reader: R) -> Result<ChunkHeader, Error>
where
    R: Read,
{
    let mut buf_4: [u8; 4] = Default::default();
    let mut buf_8: [u8; 8] = Default::default();

    reader.read_exact(&mut buf_4)?;
    let tag = Tag::from_bytes(buf_4)?;

    reader.read_exact(&mut buf_4)?;
    let flags = Flags::from_bytes(buf_4);

    reader.read_exact(&mut buf_8)?;
    let size = u64::from_be_bytes(buf_8);

    Ok(ChunkHeader { tag, flags, size })
}

/// Skips `amount` bytes in `reader`.
///
/// There's no method in [`std::io::Read`] that allows you to directly skip.
/// Instead we have to read into a buffer. We could stack-allocate one for this
/// function, but the [`Reader`] has a scratch buffer anyway, so we pass it in.
fn skip<R>(mut reader: R, amount: u64, scratch_buffer: &mut [u8]) -> Result<(), std::io::Error>
where
    R: Read,
{
    let mut amount: usize = amount.try_into().unwrap_or(usize::MAX);

    while amount > 0 {
        let n = scratch_buffer.len().min(amount);
        reader.read_exact(&mut scratch_buffer[..n])?;
        amount -= n;
    }

    Ok(())
}
