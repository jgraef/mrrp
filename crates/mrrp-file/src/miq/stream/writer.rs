use std::{
    fmt::Debug,
    fs::File,
    io::{
        BufWriter,
        Write,
    },
    marker::PhantomData,
    path::Path,
};

use serde::Serialize;

use crate::miq::{
    SUBFORMAT,
    codec::Encoder,
    container::{
        self,
        writer::BufferedChunkWriter,
    },
    stream::{
        IQ_TAG,
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
}

impl Error {
    pub fn from_encoder<E>(_error: E) -> Self {
        todo!();
    }
}

#[derive(Debug)]
pub struct Writer<W> {
    container: container::writer::Writer<W>,
    next_stream_id: u32,
}

impl<W> Writer<W>
where
    W: Write,
{
    pub fn new(writer: W) -> Result<Self, Error> {
        let mut container = container::writer::Writer::new(writer);

        // write MIQ header
        container.write_cbor_chunk(&container::header::Header {
            version: container::VERSION,
            sub_format: Some(SUBFORMAT),
            stream: false,
        })?;

        Ok(Self {
            container,
            next_stream_id: 1,
        })
    }

    pub fn start_stream<T, E>(
        &mut self,
        stream_info: StreamInfo,
        encoder: E,
    ) -> Result<StreamWriter<T, E>, Error>
    where
        T: Sample,
        E: Encoder<T>,
    {
        let stream_id = StreamId(self.next_stream_id);
        self.next_stream_id += 1;

        self.container.write_cbor_chunk(&StreamStart {
            stream_id,
            sample_format: T::SAMPLE_FORMAT,
            samples: None,
            stream_info,
        })?;

        Ok(StreamWriter {
            stream_id,
            encoder,
            _marker: PhantomData,
        })
    }

    #[inline]
    pub fn end_stream<T, E>(&mut self, stream_writer: StreamWriter<T, E>) -> Result<(), Error> {
        self.container.write_cbor_chunk(&StreamEnd {
            stream_id: stream_writer.stream_id,
            stream_info: Default::default(),
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
        self.container.write_cbor_chunk(&StreamInfoChange {
            stream_id: stream_writer.stream_id,
            stream_info,
        })?;
        Ok(())
    }

    // note: this is not pub, because we think the better API is to have
    // `begin_write` on `StreamWriter`. but from a low-level API perspective,
    // this just writes an IQ data chunk.
    fn iq_data<'a, T, E>(
        &'a mut self,
        stream_writer: &'a mut StreamWriter<T, E>,
    ) -> Result<IqWriter<'a, W, E, T>, Error> {
        // todo: this should return an IQ writer thingie. we might want that
        // thingie to have a type-parameter for the sample format. we would need
        // to put that type-param into a `StreamHandle<T>`. it would only be a
        // `PhantomData` and otherwise the `StreamHandle` would contain the
        // `StreamId`.

        let mut chunk_writer = self
            .container
            .write_chunk_buffered(IQ_TAG, Default::default());

        // write stream ID
        chunk_writer.write_all(&stream_writer.stream_id.0.to_be_bytes())?;

        Ok(IqWriter {
            chunk_writer,
            encoder: &mut stream_writer.encoder,
            _marker: PhantomData,
        })
    }
}

impl Writer<BufWriter<File>> {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::new(BufWriter::new(File::create(path)?))
    }
}

// todo: we would like to merge IqWriter into this. we can probably do this if
// we don't use the BufferedChunkWriter, but manage the buffer in StreamWriter.
// Then we can buffer here to avoid writing too few samples into a chunk. We
// also want to flush the buffer by writing a chunk whenever the number of
// buffered samples exceeds some limit.
#[derive(Debug)]
pub struct StreamWriter<T, E> {
    stream_id: StreamId,
    encoder: E,
    _marker: PhantomData<fn(&[T])>,
}

impl<T, E> StreamWriter<T, E> {
    #[inline]
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[inline]
    pub fn start_write<'a, W>(
        &'a mut self,
        writer: &'a mut Writer<W>,
    ) -> Result<IqWriter<'a, W, E, T>, Error>
    where
        W: Write,
    {
        writer.iq_data(self)
    }
}

#[derive(Debug)]
pub struct IqWriter<'a, W, E, T> {
    chunk_writer: BufferedChunkWriter<'a, W>,
    encoder: &'a mut E,
    _marker: PhantomData<fn(&[T])>,
}

impl<'a, W, E, T> IqWriter<'a, W, E, T>
where
    E: Encoder<T>,
{
    pub fn write_samples(&mut self, samples: &[T]) -> Result<(), Error> {
        self.encoder
            .write_samples(samples, &mut self.chunk_writer)
            .map_err(Error::from_encoder)
    }
}
