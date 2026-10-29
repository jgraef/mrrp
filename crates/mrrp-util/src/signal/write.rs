use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use mrrp_core::signal::AsyncWriteSamples;

use crate::signal::{
    MapErr,
    Throttled,
};

pub trait AsyncWriteSamplesExt<S>: AsyncWriteSamples<S> {
    #[inline]
    fn poll_flush_unpin(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>
    where
        Self: Unpin,
    {
        Pin::new(self).poll_flush(cx)
    }

    #[inline]
    fn poll_close_unpin(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>
    where
        Self: Unpin,
    {
        Pin::new(self).poll_close(cx)
    }

    #[inline]
    fn poll_write_samples_unpin(
        &mut self,
        cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>>
    where
        Self: Unpin,
    {
        Pin::new(self).poll_write_samples(cx, buffer)
    }

    #[inline]
    fn flush(&mut self) -> Flush<'_, Self, S>
    where
        Self: Unpin,
    {
        Flush {
            write_samples: self,
            _phantom: PhantomData,
        }
    }

    #[inline]
    fn close(&mut self) -> Close<'_, Self, S>
    where
        Self: Unpin,
    {
        Close {
            write_samples: self,
            _phantom: PhantomData,
        }
    }

    #[inline]
    fn write<'a>(&'a mut self, buffer: &'a [S]) -> Write<'a, Self, S>
    where
        Self: Unpin,
    {
        Write {
            write_samples: self,
            buffer,
        }
    }

    #[inline]
    fn write_all<'a>(&'a mut self, buffer: &'a [S]) -> WriteAll<'a, Self, S>
    where
        Self: Unpin,
    {
        WriteAll {
            write_samples: self,
            buffer,
        }
    }

    #[inline]
    fn write_sample<'a>(&'a mut self, sample: S) -> WriteSample<'a, Self, S>
    where
        Self: Unpin,
    {
        WriteSample {
            write_samples: self,
            sample,
            was_written: false,
        }
    }

    #[inline]
    fn throttled(self, sample_duration: Duration) -> Throttled<Self>
    where
        Self: Sized,
    {
        Throttled::new(self, sample_duration)
    }

    /// Maps any errors returned by the underlying sink with the provided
    /// closure.
    #[inline]
    fn map_err<E, F>(self, f: F) -> MapErr<Self, F>
    where
        F: FnMut(Self::Error) -> E,
        Self: Sized,
    {
        MapErr::new(self, f)
    }
}

impl<W, S> AsyncWriteSamplesExt<S> for W where W: AsyncWriteSamples<S> + ?Sized {}

#[derive(Debug)]
pub struct Flush<'a, W, S>
where
    W: ?Sized,
{
    write_samples: &'a mut W,
    _phantom: PhantomData<fn(&S)>,
}

impl<'a, W, S> Future for Flush<'a, W, S>
where
    W: AsyncWriteSamples<S> + Unpin + ?Sized,
{
    type Output = Result<(), W::Error>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.write_samples.poll_flush_unpin(cx)
    }
}

#[derive(Debug)]
pub struct Close<'a, W, S>
where
    W: ?Sized,
{
    write_samples: &'a mut W,
    _phantom: PhantomData<fn(&S)>,
}

impl<'a, W, S> Future for Close<'a, W, S>
where
    W: AsyncWriteSamples<S> + Unpin + ?Sized,
{
    type Output = Result<(), W::Error>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.write_samples.poll_close_unpin(cx)
    }
}

#[derive(Debug)]
pub struct Write<'a, W, S>
where
    W: ?Sized,
{
    write_samples: &'a mut W,
    buffer: &'a [S],
}

impl<'a, W, S> Future for Write<'a, W, S>
where
    W: AsyncWriteSamples<S> + Unpin + ?Sized,
{
    type Output = Result<usize, W::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        this.write_samples.poll_write_samples_unpin(cx, this.buffer)
    }
}

#[derive(Debug)]
pub struct WriteAll<'a, W, S>
where
    W: ?Sized,
{
    write_samples: &'a mut W,
    buffer: &'a [S],
}

impl<'a, W, S> Future for WriteAll<'a, W, S>
where
    W: AsyncWriteSamples<S> + Unpin + ?Sized,
{
    type Output = Result<(), W::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while !self.buffer.is_empty() {
            let this = &mut *self;

            match this.write_samples.poll_write_samples_unpin(cx, this.buffer) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(num_samples_written)) => {
                    this.buffer = &this.buffer[num_samples_written..];
                }
            }
        }

        Poll::Ready(Ok(()))
    }
}

#[derive(Debug)]
pub struct WriteSample<'a, W, S>
where
    W: ?Sized,
{
    write_samples: &'a mut W,
    sample: S,
    was_written: bool,
}

impl<'a, W, S> Future for WriteSample<'a, W, S>
where
    W: AsyncWriteSamples<S> + Unpin + ?Sized,
    S: Clone + Unpin,
{
    type Output = Result<(), W::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while !self.was_written {
            let this = &mut *self;
            match this
                .write_samples
                .poll_write_samples_unpin(cx, &[this.sample.clone()])
            {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(num_samples_written)) => {
                    assert!(num_samples_written <= 1);
                    this.was_written = num_samples_written == 1;
                    break;
                }
            }
        }

        Poll::Ready(Ok(()))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NullSink;

impl<S> AsyncWriteSamples<S> for NullSink {
    type Error = !;

    fn poll_write_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        _buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        // we always return pending, so we don't need to use `tokio::task::coop`
        // here. <https://github.com/jgraef/mrrp/issues/30>
        let _ = cx;
        Poll::Pending
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[inline]
pub fn null_sink() -> NullSink {
    NullSink
}
