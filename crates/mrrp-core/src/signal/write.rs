use std::{
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
