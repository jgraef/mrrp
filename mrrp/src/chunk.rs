use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::{
    Sink,
    Stream,
    StreamExt,
};

use crate::{
    GetSampleRate,
    buf::{
        SampleBuf,
        SampleBufMut,
        SamplesMut,
    },
    io::{
        AsyncReadSamples,
        AsyncWriteSamples,
        AsyncWriteSamplesExt,
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
    #[inline]
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
    E: std::error::Error,
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

impl<T, C, S, E> Stream for ChunkStreamReadSamples<T, C, S, E>
where
    T: Stream<Item = Result<C, E>> + Unpin,
    C: Unpin,
{
    type Item = Result<C, E>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(chunk) = self.chunk.take() {
            Poll::Ready(Some(Ok(chunk)))
        }
        else {
            self.stream.poll_next_unpin(cx)
        }
    }
}

impl<T, C, S, E> GetSampleRate for ChunkStreamReadSamples<T, C, S, E>
where
    T: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.stream.sample_rate()
    }
}

#[derive(Clone, Debug)]
pub struct ReadSamplesChunkStream<R, S> {
    pub read_samples: R,
    pub chunk_size: usize,
    _phantom: PhantomData<fn() -> SamplesMut<S>>,
}

impl<R, S> ReadSamplesChunkStream<R, S> {
    #[inline]
    pub fn new(read_samples: R, chunk_size: usize) -> Self {
        Self {
            read_samples,
            chunk_size,
            _phantom: PhantomData,
        }
    }
}

impl<R, S> Stream for ReadSamplesChunkStream<R, S>
where
    R: AsyncReadSamples<S> + Unpin,
{
    type Item = Result<SamplesMut<S>, R::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut buffer = SamplesMut::with_capacity(self.chunk_size);
        let mut read_buf = ReadBuf::uninit(buffer.chunk_mut());

        let this = &mut *self;

        match Pin::new(&mut this.read_samples).poll_read_samples(cx, &mut read_buf) {
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

impl<R, S> GetSampleRate for ReadSamplesChunkStream<R, S>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.read_samples.sample_rate()
    }
}

#[derive(Clone, Debug)]
pub struct WriteSamplesChunkSink<W, C, S> {
    pub write_samples: W,
    chunk: Option<C>,
    // todo: what is the right variance here?
    _phantom: PhantomData<fn(S)>,
}

impl<W, C, S> WriteSamplesChunkSink<W, C, S> {
    #[inline]
    pub fn new(write_samples: W) -> Self {
        Self {
            write_samples,
            chunk: None,
            _phantom: PhantomData,
        }
    }
}

impl<W, C, S> Sink<C> for WriteSamplesChunkSink<W, C, S>
where
    W: AsyncWriteSamples<S> + Unpin,
    C: SampleBuf<S> + Unpin,
{
    type Error = W::Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        let this = &mut *self;

        if let Some(chunk) = &mut this.chunk {
            match this
                .write_samples
                .poll_write_samples_unpin(cx, chunk.chunk())
            {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(num_samples_written)) => {
                    chunk.advance(num_samples_written);
                    Poll::Ready(Ok(()))
                }
            }
        }
        else {
            Poll::Ready(Ok(()))
        }
    }

    #[inline]
    fn start_send(mut self: Pin<&mut Self>, item: C) -> Result<(), Self::Error> {
        self.chunk = Some(item);
        Ok(())
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.write_samples.poll_flush_unpin(cx)
    }

    #[inline]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.write_samples.poll_close_unpin(cx)
    }
}
