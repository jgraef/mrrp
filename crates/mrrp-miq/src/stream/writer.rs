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

use bytes::{
    Buf,
    BytesMut,
};
use mrrp_core::{
    buf::{
        SampleBuf,
        TryAdvanceError,
    },
    sample::encoding::{
        BigEndian,
        Bytes,
        Encode,
    },
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
        })
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

        Ok(StreamWriter::new(stream_id, encoder))
    }

    #[inline]
    pub fn end_stream<T, E>(&mut self, stream_writer: StreamWriter<T, E>) -> Result<(), Error> {
        self.container_writer.write_cbor_chunk(&StreamEnd::<()> {
            stream_id: stream_writer.stream_id,
            stream_info: Default::default(),
            checksum: stream_writer.checksum(),
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

#[derive(derive_more::Debug)]
pub struct StreamWriter<T, E> {
    stream_id: StreamId,
    encoder: E,

    #[debug("<{} bytes>", buffer.len())]
    buffer: BytesMut,

    checksum_hasher: Option<ChecksumHasher<T>>,

    stream_position: usize,
    chunk_stream_position: usize,

    #[debug(skip)]
    _marker: PhantomData<fn(&[T])>,
}

impl<T, E> StreamWriter<T, E> {
    #[inline]
    fn new(stream_id: StreamId, encoder: E) -> Self {
        Self {
            stream_id,
            encoder,
            buffer: BytesMut::new(),
            checksum_hasher: Default::default(),
            stream_position: 0,
            chunk_stream_position: 0,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[inline]
    pub fn checksum(&self) -> Option<u64> {
        self.checksum_hasher
            .as_ref()
            .map(|checksum_hasher| checksum_hasher.finish())
    }

    pub fn stream_position(&self) -> usize {
        self.stream_position
    }
}

impl<E, T> StreamWriter<T, E>
where
    E: Encoder<T>,
    T: Encode<BigEndian>,
{
    /// Write samples into stream
    pub fn write_samples<'a, W, B>(
        &mut self,
        writer: &'a mut Writer<W>,
        samples: B,
    ) -> Result<(), Error>
    where
        W: Write,
        B: SampleBuf<T>,
    {
        // wrap SampleBuf so that anything read from it will be hashed and the
        // current sample_num will be tracked.
        let samples = WriterSamplesBufWrapper {
            inner: samples,
            hasher: self.checksum_hasher.as_mut(),
            stream_position: &mut self.stream_position,
        };

        // push samples into encoder
        let flush = self
            .encoder
            .write_samples(samples, &mut self.buffer)
            .map_err(Error::from_encoder)?;

        // flush if encoder wants us to
        if matches!(flush, Flush::Flush) {
            let header = u64::try_from(self.chunk_stream_position)
                .unwrap()
                .to_be_bytes();

            writer.container_writer.write_data_chunk(
                IQ_TAG,
                Default::default(),
                Buf::chain(header.as_slice(), &*self.buffer),
            )?;

            self.buffer.clear();
            self.chunk_stream_position = self.stream_position;
        }

        Ok(())
    }
}

/// Checksum from hashing samples as PCM using [seahash][1]. Encoding is done
/// using big endian where applicable.
///
/// [1]: https://docs.rs/seahash/latest/seahash/index.html
#[derive(derive_more::Debug)]
pub struct ChecksumHasher<T> {
    #[debug("SeaHasher {{ ... }}")]
    hasher: SeaHasher,

    #[debug(skip)]
    _marker: PhantomData<fn(T)>,
}

impl<T> ChecksumHasher<T> {
    /// Seed for the seahash
    pub const SEED: [u64; 4] = [
        0x7d71d5fdfa9a9e36,
        0xd1d329a80c99931e,
        0x8b55a32b5757926c,
        0x34d490e72e8c727d,
    ];

    #[inline]
    pub fn finish(&self) -> u64 {
        self.hasher.finish()
    }
}

impl<T> ChecksumHasher<T>
where
    T: Encode<BigEndian>,
{
    pub fn update(&mut self, samples: &[T]) {
        for sample in samples {
            self.hasher.write(sample.encode().as_slice());
        }
    }
}

impl<T> Default for ChecksumHasher<T> {
    #[inline]
    fn default() -> Self {
        Self {
            hasher: SeaHasher::with_seeds(
                Self::SEED[0],
                Self::SEED[1],
                Self::SEED[2],
                Self::SEED[3],
            ),
            _marker: PhantomData,
        }
    }
}

/// Wrapper for a [`SampleBuf`] that hashes anything that is consumed and keeps
/// track of the sample number.
///
/// This is a helper used by [`StreamWriter`].
struct WriterSamplesBufWrapper<'a, B, T>
where
    B: SampleBuf<T>,
{
    inner: B,
    hasher: Option<&'a mut ChecksumHasher<T>>,
    stream_position: &'a mut usize,
}

impl<'a, B, T> SampleBuf<T> for WriterSamplesBufWrapper<'a, B, T>
where
    B: SampleBuf<T>,
    T: Encode<BigEndian>,
{
    fn try_advance(&mut self, mut amount: usize) -> Result<(), TryAdvanceError> {
        if self.inner.remaining() < amount {
            return Err(TryAdvanceError {
                requested: amount,
                available: self.inner.remaining(),
            });
        }

        while amount > 0 {
            let chunk = self.inner.chunk();
            assert!(!chunk.is_empty());
            let amount_in_this_chunk = amount.min(chunk.len());

            if let Some(hasher) = &mut self.hasher {
                hasher.update(&chunk[..amount_in_this_chunk]);
            }

            *self.stream_position += amount_in_this_chunk;
            self.inner.advance(amount_in_this_chunk);
            amount -= amount_in_this_chunk;
        }

        Ok(())
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }

    #[inline]
    fn chunk(&self) -> &[T] {
        self.inner.chunk()
    }
}
