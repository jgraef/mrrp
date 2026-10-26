use std::{
    fmt::Debug,
    fs::File,
    hash::Hasher,
    io::{
        BufWriter,
        Write,
    },
    marker::PhantomData,
    path::Path,
};

use seahash::SeaHasher;
use serde::Serialize;

use crate::{
    codec::{
        Encoder,
        Flush,
    },
    container,
    stream::{
        IQ_TAG,
        SUB_FORMAT,
        Sample,
        StreamEnd,
        StreamId,
        StreamInfo,
        StreamInfoChange,
        StreamStart,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Container(#[from] container::writer::Error),

    #[error("{0}")]
    Encoder(#[source] Box<dyn std::error::Error + Send + Sync>),
}

impl Error {
    #[inline]
    pub fn from_encoder<E>(error: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self::Encoder(Box::new(error))
    }
}

pub const DEFAULT_MIN_CHUNK_SIZE: usize = 0x1000;

#[derive(Debug)]
pub struct Writer<W> {
    container_writer: container::writer::Writer<W>,
    next_stream_id: u32,

    /// See [`StreamWriter`].
    min_chunk_size: usize,
}

impl<W> Writer<W>
where
    W: Write,
{
    pub fn new(writer: W) -> Result<Self, Error> {
        let mut container_writer = container::writer::Writer::new(writer);

        // write MIQ header
        container_writer.write_cbor_chunk(&container::header::FileHeader {
            version: container::VERSION,
            sub_format: Some(SUB_FORMAT),
            stream: false,
        })?;

        Ok(Self {
            container_writer,
            next_stream_id: 1,
            min_chunk_size: DEFAULT_MIN_CHUNK_SIZE,
        })
    }

    #[inline]
    pub fn set_min_chunk_size(&mut self, min_chunk_size: usize) {
        self.min_chunk_size = min_chunk_size;
    }

    pub fn start_stream<T, E, U>(
        &mut self,
        stream_info: StreamInfo<U>,
        encoder: E,
    ) -> Result<StreamWriter<T, E>, Error>
    where
        T: Sample,
        E: Encoder<T>,
        U: Serialize,
    {
        let stream_id = StreamId(self.next_stream_id);
        self.next_stream_id += 1;

        self.container_writer.write_cbor_chunk(&StreamStart {
            stream_id,
            sample_format: T::SAMPLE_FORMAT,
            num_channels: Default::default(),
            samples: None,
            stream_info,
        })?;

        Ok(StreamWriter::new(stream_id, encoder, self.min_chunk_size))
    }

    #[inline]
    pub fn end_stream<T, E>(&mut self, stream_writer: StreamWriter<T, E>) -> Result<(), Error> {
        self.container_writer.write_cbor_chunk(&StreamEnd::<()> {
            stream_id: stream_writer.stream_id,
            stream_info: Default::default(),
            // todo
            //checksum: Some(stream_writer.checksum()),
            checksum: {
                let _ = stream_writer.checksum();
                None
            },
        })?;
        Ok(())
    }

    #[inline]
    pub fn update_stream_info<T, E, U>(
        &mut self,
        stream_writer: &StreamWriter<T, E>,
        stream_info: StreamInfo<U>,
    ) -> Result<(), Error>
    where
        U: Serialize,
    {
        self.container_writer.write_cbor_chunk(&StreamInfoChange {
            stream_id: stream_writer.stream_id,
            stream_info,
        })?;
        Ok(())
    }
}

impl Writer<BufWriter<File>> {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::new(BufWriter::new(File::create(path)?))
    }
}

pub const CHECKSUM_SEED: [u64; 4] = [
    0x7d71d5fdfa9a9e36,
    0xd1d329a80c99931e,
    0x8b55a32b5757926c,
    0x34d490e72e8c727d,
];

// todo: we would like to merge IqWriter into this. we can probably do this if
// we don't use the BufferedChunkWriter, but manage the buffer in StreamWriter.
// Then we can buffer here to avoid writing too few samples into a chunk. We
// also want to flush the buffer by writing a chunk whenever the number of
// buffered samples exceeds some limit.
#[derive(derive_more::Debug)]
pub struct StreamWriter<T, E> {
    stream_id: StreamId,
    encoder: E,

    #[debug("<{} bytes>", buffer.len())]
    buffer: Vec<u8>,

    /// If the underlying encoder supports flushing whenever we want, we will
    /// flush once the buffer is at least `min_chunk_size` bytes full.
    min_chunk_size: usize,

    #[debug("SeaHasher {{ ... }}")]
    checksum_hasher: SeaHasher,

    #[debug(skip)]
    _marker: PhantomData<fn(&[T])>,
}

impl<T, E> StreamWriter<T, E> {
    #[inline]
    fn new(stream_id: StreamId, encoder: E, min_chunk_size: usize) -> Self {
        Self {
            stream_id,
            encoder,
            buffer: Vec::with_capacity(min_chunk_size),
            min_chunk_size,
            checksum_hasher: SeaHasher::with_seeds(
                CHECKSUM_SEED[0],
                CHECKSUM_SEED[1],
                CHECKSUM_SEED[2],
                CHECKSUM_SEED[3],
            ),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[inline]
    pub fn checksum(&self) -> u64 {
        let _ = self.checksum_hasher.finish();
        todo!("push samples into checksum hasher");
    }
}

impl<E, T> StreamWriter<T, E>
where
    E: Encoder<T>,
{
    pub fn write_samples<'a, W>(
        &mut self,
        writer: &'a mut Writer<W>,
        samples: &[T],
    ) -> Result<(), Error>
    where
        W: Write,
    {
        // todo: push samples into checksum hasher

        let flush = self
            .encoder
            .write_samples(samples, &mut self.buffer)
            .map_err(Error::from_encoder)?;

        let flush = match flush {
            Flush::Buffer => {
                // don't flush
                false
            }
            Flush::Flush => {
                // must flush now
                true
            }
            Flush::Any => {
                // we may flush now. check if the buffer is already larger than
                // some configurable limit
                self.buffer.len() >= self.min_chunk_size
            }
        };

        if flush {
            writer
                .container_writer
                .write_data_chunk(IQ_TAG, Default::default(), &self.buffer)?;

            self.buffer.clear();
        }

        Ok(())
    }
}
