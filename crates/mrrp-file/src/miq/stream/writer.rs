use std::{
    fmt::Debug,
    io::Write,
    marker::PhantomData,
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
        SampleFormat,
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

    pub fn start_stream<E>(
        &mut self,
        sample_format: SampleFormat,
        stream_info: StreamInfo,
        encoder: E,
    ) -> Result<StreamWriter<E>, Error> {
        let stream_id = StreamId(self.next_stream_id);
        self.next_stream_id += 1;

        self.container.write_cbor_chunk(&StreamStart {
            stream_id,
            sample_format,
            samples: None,
            stream_info,
        })?;

        Ok(StreamWriter { stream_id, encoder })
    }

    #[inline]
    pub fn end_stream<E>(&mut self, stream_writer: StreamWriter<E>) -> Result<(), Error> {
        self.container.write_cbor_chunk(&StreamEnd {
            stream_id: stream_writer.stream_id,
        })?;
        Ok(())
    }

    #[inline]
    pub fn update_stream_info<E, U>(
        &mut self,
        stream_writer: &StreamWriter<E>,
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

    pub fn iq_data<'a, E, T>(
        &'a mut self,
        stream_writer: &'a mut StreamWriter<E>,
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

#[derive(Debug)]
pub struct StreamWriter<E> {
    stream_id: StreamId,
    encoder: E,
}

impl<E> StreamWriter<E> {
    #[inline]
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
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
