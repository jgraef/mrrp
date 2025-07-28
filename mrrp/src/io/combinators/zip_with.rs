use std::{
    marker::PhantomData,
    pin::Pin,
    task::{
        Context,
        Poll,
        ready,
    },
};

use pin_project_lite::pin_project;

use crate::{
    buf::SampleBufMut,
    io::{
        AsyncReadSamples,
        Buffer,
        FiniteStream,
        GetSampleRate,
        ReadBuf,
        Remaining,
        StreamLength,
        combinators::Scanner,
    },
};

pin_project! {
    // note: the left stream is authorative on sample rate and finitness. this is not ideal
    #[derive(Clone, Debug)]
    pub struct ZipWith<L, R, S, T, Sc> {
        #[pin]
        left_stream: L,
        left_buffer: Buffer<S>,
        #[pin]
        right_stream: R,
        right_buffer: Buffer<T>,
        scanner: Sc,
        _phantom: PhantomData<fn(S, T)>
    }
}

impl<L, R, S, T, Sc> ZipWith<L, R, S, T, Sc> {
    #[inline]
    pub fn new(left: L, right: R, scanner: Sc) -> Self {
        Self {
            left_stream: left,
            left_buffer: Buffer::default(),
            right_stream: right,
            right_buffer: Buffer::default(),
            scanner,
            _phantom: PhantomData,
        }
    }
}

impl<L, R, S, T, Sc> AsyncReadSamples<Sc::Output> for ZipWith<L, R, S, T, Sc>
where
    L: AsyncReadSamples<S>,
    R: AsyncReadSamples<T>,
    Sc: Scanner<(S, T)>,
{
    type Error = ZipError<L::Error, R::Error>;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Sc::Output>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        let n = buffer.remaining();

        if n == 0 {
            return Poll::Ready(Ok(()));
        }

        this.left_buffer.grow(n);
        this.right_buffer.grow(n);

        let num_samples_left =
            ready!(this.left_buffer.poll_fill(cx, this.left_stream)).map_err(ZipError::Left)?;
        let num_samples_right =
            ready!(this.right_buffer.poll_fill(cx, this.right_stream)).map_err(ZipError::Right)?;

        let num_samples = num_samples_left.min(num_samples_right);

        for (left_sample, right_sample) in this
            .left_buffer
            .drain(num_samples)
            .zip(this.right_buffer.drain(num_samples))
        {
            buffer.put_sample(this.scanner.scan((left_sample, right_sample)));
        }

        Poll::Ready(Ok(()))
    }
}

impl<L, R, S, T, Sc> GetSampleRate for ZipWith<L, R, S, T, Sc>
where
    L: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.left_stream.sample_rate()
    }
}

impl<L, R, S, T, Sc> StreamLength for ZipWith<L, R, S, T, Sc>
where
    L: StreamLength,
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.left_stream
            .remaining()
            .min(self.right_stream.remaining())
    }
}

impl<L, R, S, T, Sc> FiniteStream for ZipWith<L, R, S, T, Sc> where L: FiniteStream {}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("zip stream error")]
pub enum ZipError<L, R> {
    Left(L),
    Right(R),
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use crate::io::{
        AsyncReadSamplesExt,
        Cursor,
        Remaining,
        StreamLength,
        combinators::FuncScanner,
        test::SingleSampleStream,
    };

    #[test]
    fn it_adds_two_streams() {
        let left = Cursor::new((0..100).collect::<Vec<_>>());
        let right = Cursor::new((0..100).map(|x| x * x).collect::<Vec<_>>());

        let mut summed = vec![];
        left.zip_with(right, FuncScanner::new(|(a, b)| a + b))
            .read_to_end(&mut summed)
            .now_or_never()
            .expect("pending")
            .unwrap();

        assert_eq!(summed.len(), 100);
        summed.iter().enumerate().for_each(|(i, sample)| {
            let expected = i + i * i;
            assert_eq!(*sample, expected as i32);
        });
    }

    #[test]
    fn it_zips_streams_with_different_length_chunks() {
        let left = SingleSampleStream::new(Cursor::new((0..100).collect::<Vec<_>>()));
        let right = Cursor::new((0..100).map(|x| x * x).collect::<Vec<_>>());

        let mut summed = vec![];
        left.zip_with(right, FuncScanner::new(|(a, b)| a + b))
            .read_to_end(&mut summed)
            .now_or_never()
            .expect("pending")
            .unwrap();

        assert_eq!(summed.len(), 100);
        summed.iter().enumerate().for_each(|(i, sample)| {
            let expected = i + i * i;
            assert_eq!(*sample, expected as i32);
        });
    }

    #[test]
    fn it_zips_streams_with_different_lengths() {
        let left = Cursor::new((0..50).collect::<Vec<_>>());
        let right = Cursor::new((0..100).map(|x| x * x).collect::<Vec<_>>());

        let mut stream = left.zip_with(right, FuncScanner::new(|(a, b)| a + b));

        assert_eq!(stream.remaining(), Remaining::Finite { num_samples: 50 });

        let mut summed = vec![];
        stream
            .read_to_end(&mut summed)
            .now_or_never()
            .expect("pending")
            .unwrap();

        assert_eq!(summed.len(), 50);
        summed.iter().enumerate().for_each(|(i, sample)| {
            let expected = i + i * i;
            assert_eq!(*sample, expected as i32);
        });
    }
}
