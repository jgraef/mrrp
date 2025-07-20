use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

pub trait AsyncWriteSamples<S> {
    type Error;

    fn poll_write_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>>;

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;
}

impl<W, S> AsyncWriteSamples<S> for &mut W
where
    W: AsyncWriteSamples<S> + Unpin + ?Sized,
{
    type Error = W::Error;

    #[inline]
    fn poll_write_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[S],
    ) -> Poll<Result<usize, Self::Error>> {
        Pin::new(&mut **self).poll_write_samples(cx, buffer)
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut **self).poll_flush(cx)
    }

    #[inline]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut **self).poll_close(cx)
    }
}

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
