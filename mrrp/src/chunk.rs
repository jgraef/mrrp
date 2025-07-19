use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::{
    Stream,
    StreamExt,
};

use crate::{
    buf::{
        SampleBuf,
        SampleBufMut,
        SamplesMut,
    },
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        ReadBuf,
    },
};

#[derive(Clone, Debug)]
pub struct ChunkStreamReadSamples<T, C, S, E> {
    pub stream: T,
    chunk: Option<C>,
    _phantom: PhantomData<fn() -> Result<S, E>>,
}

impl<T, C, S, E> ChunkStreamReadSamples<T, C, S, E> {
    pub fn new(stream: T) -> Self {
        Self {
            stream,
            chunk: None,
            _phantom: PhantomData,
        }
    }
}

impl<T, C, S, E> AsyncReadSamples<S> for ChunkStreamReadSamples<T, C, S, E>
where
    T: Stream<Item = Result<C, E>> + Unpin,
    C: SampleBuf<S> + Unpin,
    S: Clone,
{
    type Error = E;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            let this = &mut *self;

            if let Some(chunk) = &mut this.chunk {
                buffer.put(chunk.take(buffer.remaining()));

                if !chunk.has_remaining() {
                    this.chunk = None;
                }

                return Poll::Ready(Ok(()));
            }
            else {
                match this.stream.poll_next_unpin(cx) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(None) => return Poll::Ready(Ok(())),
                    Poll::Ready(Some(Err(error))) => return Poll::Ready(Err(error)),
                    Poll::Ready(Some(Ok(chunk))) => {
                        this.chunk = Some(chunk);
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct ReadSamplesChunkStream<T, S> {
    pub read_samples: T,
    pub chunk_size: usize,
    _phantom: PhantomData<fn() -> SamplesMut<S>>,
}

impl<T, S> ReadSamplesChunkStream<T, S> {
    pub fn new(read_samples: T, chunk_size: usize) -> Self {
        Self {
            read_samples,
            chunk_size,
            _phantom: PhantomData,
        }
    }
}

impl<T, S> Stream for ReadSamplesChunkStream<T, S>
where
    T: AsyncReadSamples<S> + Unpin,
{
    type Item = Result<SamplesMut<S>, T::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buffer = SamplesMut::with_capacity(self.chunk_size);
        let mut read_buf = ReadBuf::uninit(buffer.chunk_mut());

        let this = &mut *self;

        match this.read_samples.poll_read_samples_unpin(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Some(Err(error))),
            Poll::Ready(Ok(())) => {
                let num_samples_read = read_buf.filled().len();
                unsafe {
                    buffer.advance_mut(num_samples_read);
                }
                Poll::Ready(Some(Ok(buffer)))
            }
        }
    }
}
