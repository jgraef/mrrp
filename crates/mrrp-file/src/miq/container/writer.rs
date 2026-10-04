use std::io::{
    Cursor,
    Write,
};

use serde::Serialize;

use crate::miq::container::chunk::{
    ChunkHeader,
    Flags,
    Tag,
    Tagged,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("CBOR error: {message}")]
    Cbor { message: String },
}

impl From<ciborium::ser::Error<std::io::Error>> for Error {
    fn from(value: ciborium::ser::Error<std::io::Error>) -> Self {
        match value {
            ciborium::ser::Error::Io(error) => {
                // note: at time of writing this case should not happen, because
                // we're serializing into a `Vec<u8>`, which
                // does not return `Err`. Because ciborium uses
                // the underlying `Write` impl on `Vec`, it does
                // return results with `std::io::Error` though.
                Self::Io(error)
            }
            ciborium::ser::Error::Value(message) => Self::Cbor { message },
        }
    }
}

#[derive(Debug)]
pub struct Writer<W> {
    writer: W,
    buffer: Vec<u8>,
}

impl<W> Writer<W> {
    pub fn new(writer: W) -> Self {
        Self {
            writer,
            buffer: vec![],
        }
    }
}

impl<W> Writer<W>
where
    W: Write,
{
    /// Write a chunk with a buffered writer.
    ///
    /// This returns a [`BufferedChunkWriter`] that implements [`Write`].
    #[inline]
    #[must_use = "call `.finish()` to write the chunk"]
    pub fn write_chunk_buffered(&mut self, tag: Tag, flags: Flags) -> BufferedChunkWriter<'_, W> {
        BufferedChunkWriter::new(self, tag, flags)
    }

    /// Write a chunk from a slice.
    ///
    /// This is unbuffered. It first writes the header, using the size of the
    /// slice for the `size` field. Then it writes the data directly.
    #[inline]
    pub fn write_data_chunk(&mut self, tag: Tag, flags: Flags, data: &[u8]) -> Result<(), Error> {
        write_chunk(&mut self.writer, tag, flags, data)?;
        Ok(())
    }

    /// Writes a structured chunk.
    ///
    /// Structured chunks a CBOR encoded. This creates a
    /// [`BufferedChunkWriter`], encodes the `value` into it, and flushes it to
    /// the underlying writer.
    pub fn write_cbor_chunk<T>(&mut self, value: &T) -> Result<(), Error>
    where
        T: Tagged + Serialize,
    {
        // we serialize into a in-memory buffer first, so that we know the size
        // of the chunk and don't have to seek back to write it.
        let mut chunk_writer = self.write_chunk_buffered(T::TAG, Flags::CBOR);
        ciborium::into_writer(value, &mut chunk_writer)?;
        chunk_writer.finish()?;

        Ok(())
    }
}

/// [`Write`] adapter to write a chunk.
///
/// You must call [`finish`][Self::finish] to actually write the chunk.
#[derive(Debug)]
pub struct BufferedChunkWriter<'a, W> {
    writer: &'a mut W,
    buffer: Cursor<&'a mut Vec<u8>>,
    pub tag: Tag,
    pub flags: Flags,
}

impl<'a, W> BufferedChunkWriter<'a, W> {
    #[must_use = "call `.finish()` to write the chunk"]
    fn new(writer: &'a mut Writer<W>, tag: Tag, flags: Flags) -> Self {
        writer.buffer.clear();
        Self {
            writer: &mut writer.writer,
            buffer: Cursor::new(&mut writer.buffer),
            tag,
            flags,
        }
    }
}

impl<'a, W> BufferedChunkWriter<'a, W>
where
    W: Write,
{
    /// Writes the buffered chunk to the underlying writer.
    pub fn finish(mut self) -> Result<(), Error> {
        let buffer = self.buffer.into_inner();
        write_chunk(&mut self.writer, self.tag, self.flags, &buffer)?;
        buffer.clear();
        Ok(())
    }
}

impl<'a, W> Write for BufferedChunkWriter<'a, W> {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buffer.write(buf)
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        // note: at the time of writing, this is a NOP
        self.buffer.flush()
    }
}

/// Low-level function to write a [`ChunkHeader`] to a [`Write`].
pub fn write_chunk_header<W>(
    mut writer: W,
    chunk_header: &ChunkHeader,
) -> Result<(), std::io::Error>
where
    W: Write,
{
    writer.write_all(&chunk_header.tag.to_bytes())?;
    writer.write_all(&chunk_header.flags.to_bytes())?;
    writer.write_all(&chunk_header.size.to_be_bytes())?;
    Ok(())
}

/// Low-level function to write a chunk to a [`Write`].
pub fn write_chunk<W>(
    mut writer: W,
    tag: Tag,
    flags: Flags,
    data: &[u8],
) -> Result<(), std::io::Error>
where
    W: Write,
{
    write_chunk_header(
        &mut writer,
        &ChunkHeader {
            tag,
            flags,
            size: data.len().try_into().expect("chunk size overflow"),
        },
    )?;
    writer.write_all(data)?;
    Ok(())
}
