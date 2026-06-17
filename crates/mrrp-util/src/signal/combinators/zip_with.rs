use std::{
    fmt::Debug,
    ops::{
        Add,
        Mul,
    },
    pin::Pin,
    task::{
        Context,
        Poll,
        ready,
    },
};

use mrrp_core::{
    buf::SampleBufMut,
    signal::{
        FiniteStream,
        GetSampleRate,
        Remaining,
        SizeHint,
        StreamLength,
    },
};
use pin_project_lite::pin_project;

use crate::signal::{
    AsyncReadSamples,
    Buffer,
    ReadBuf,
    combinators::{
        Scanner,
        scan::{
            ProductScanner,
            SumScanner,
        },
    },
};

pin_project! {
    // note: the left stream is authorative on sample rate and finitness. this is not ideal
    pub struct ZipWith<L, R, Sc>
    where
        L: AsyncReadSamples,
        R: AsyncReadSamples,
    {
        #[pin]
        left_stream: L,
        left_buffer: Buffer<L::Sample>,
        #[pin]
        right_stream: R,
        right_buffer: Buffer<R::Sample>,
        scanner: Sc,
    }
}

impl<L, R, Sc> ZipWith<L, R, Sc>
where
    L: AsyncReadSamples,
    R: AsyncReadSamples,
{
    #[inline]
    pub fn new(left: L, right: R, scanner: Sc) -> Self {
        Self {
            left_stream: left,
            left_buffer: Buffer::default(),
            right_stream: right,
            right_buffer: Buffer::default(),
            scanner,
        }
    }
}

impl<L, R, Sc> AsyncReadSamples for ZipWith<L, R, Sc>
where
    L: AsyncReadSamples,
    R: AsyncReadSamples,
    Sc: Scanner<(L::Sample, R::Sample)>,
{
    type Sample = Sc::Output;
    type Error = ZipError<L::Error, R::Error>;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
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

// todo: should we keep this?
impl<L, R, Sc> GetSampleRate for ZipWith<L, R, Sc>
where
    L: AsyncReadSamples + GetSampleRate,
    R: AsyncReadSamples,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.left_stream.sample_rate()
    }
}

impl<L, R, Sc> StreamLength for ZipWith<L, R, Sc>
where
    L: AsyncReadSamples + StreamLength,
    R: AsyncReadSamples + StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        let left_remaining = self.left_stream.remaining() + self.left_buffer.remaining();
        let right_remaining = self.right_stream.remaining() + self.left_buffer.remaining();
        left_remaining.min(right_remaining)
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        let left_size_hint = self.left_stream.size_hint() + self.left_buffer.remaining();
        let right_size_hint = self.right_stream.size_hint() + self.right_buffer.remaining();
        left_size_hint.min(right_size_hint)
    }
}

impl<L, R, Sc> FiniteStream for ZipWith<L, R, Sc>
where
    L: AsyncReadSamples + FiniteStream,
    R: AsyncReadSamples + FiniteStream,
{
}

impl<L, R, Sc> Clone for ZipWith<L, R, Sc>
where
    L: AsyncReadSamples + Clone,
    R: AsyncReadSamples + Clone,
    Sc: Clone,
    L::Sample: Clone,
    R::Sample: Clone,
{
    fn clone(&self) -> Self {
        Self {
            left_stream: self.left_stream.clone(),
            left_buffer: self.left_buffer.clone(),
            right_stream: self.right_stream.clone(),
            right_buffer: self.right_buffer.clone(),
            scanner: self.scanner.clone(),
        }
    }
}

impl<L, R, Sc> Debug for ZipWith<L, R, Sc>
where
    L: AsyncReadSamples + Debug,
    R: AsyncReadSamples + Debug,
    Sc: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZipWith")
            .field("left_stream", &self.left_stream)
            .field("left_buffer", &self.left_buffer)
            .field("right_stream", &self.right_stream)
            .field("right_buffer", &self.right_buffer)
            .field("scanner", &self.scanner)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("zip stream error")]
pub enum ZipError<L, R> {
    Left(L),
    Right(R),
}

pin_project! {
    // todo: rename to Superposition?
    #[derive(Debug)]
    pub struct Summed<L, R>
    where
        L: AsyncReadSamples,
        R: AsyncReadSamples,
    {
        #[pin]
        inner: ZipWith<L, R, SumScanner>,
    }
}

impl<L, R> Summed<L, R>
where
    L: AsyncReadSamples,
    R: AsyncReadSamples,
{
    #[inline]
    pub fn new(left: L, right: R) -> Self {
        Self {
            inner: ZipWith::new(left, right, SumScanner),
        }
    }
}

impl<L, R> AsyncReadSamples for Summed<L, R>
where
    L: AsyncReadSamples,
    R: AsyncReadSamples,
    L::Sample: Add<R::Sample>,
{
    type Sample = <L::Sample as Add<R::Sample>>::Output;
    type Error = ZipError<L::Error, R::Error>;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_read_samples(cx, buffer)
    }
}

impl<L, R> GetSampleRate for Summed<L, R>
where
    L: AsyncReadSamples + GetSampleRate,
    R: AsyncReadSamples,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<L, R> StreamLength for Summed<L, R>
where
    L: AsyncReadSamples + StreamLength,
    R: AsyncReadSamples + StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<L, R> FiniteStream for Summed<L, R>
where
    L: AsyncReadSamples + FiniteStream,
    R: AsyncReadSamples + FiniteStream,
{
}

impl<L, R> Clone for Summed<L, R>
where
    L: AsyncReadSamples + Clone,
    R: AsyncReadSamples + Clone,
    L::Sample: Clone,
    R::Sample: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

pin_project! {
    // todo: renamed to Mixed?
    pub struct Multiplied<L, R>
    where
        L: AsyncReadSamples,
        R: AsyncReadSamples,
    {
        #[pin]
        inner: ZipWith<L, R, ProductScanner>,
    }
}

impl<L, R> Multiplied<L, R>
where
    L: AsyncReadSamples,
    R: AsyncReadSamples,
{
    #[inline]
    pub fn new(left: L, right: R) -> Self {
        Self {
            inner: ZipWith::new(left, right, ProductScanner),
        }
    }
}

impl<L, R> AsyncReadSamples for Multiplied<L, R>
where
    L: AsyncReadSamples,
    R: AsyncReadSamples,
    L::Sample: Mul<R::Sample>,
{
    type Sample = <L::Sample as Mul<R::Sample>>::Output;
    type Error = ZipError<L::Error, R::Error>;

    #[inline]
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_read_samples(cx, buffer)
    }
}

impl<L, R> GetSampleRate for Multiplied<L, R>
where
    L: AsyncReadSamples + GetSampleRate,
    R: AsyncReadSamples,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<L, R> StreamLength for Multiplied<L, R>
where
    L: AsyncReadSamples + StreamLength,
    R: AsyncReadSamples + StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<L, R> FiniteStream for Multiplied<L, R>
where
    L: AsyncReadSamples + FiniteStream,
    R: AsyncReadSamples + FiniteStream,
{
}

impl<L, R> Clone for Multiplied<L, R>
where
    L: AsyncReadSamples + Clone,
    R: AsyncReadSamples + Clone,
    L::Sample: Clone,
    R::Sample: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;
    use mrrp_core::signal::{
        Remaining,
        StreamLength,
    };

    use crate::signal::{
        AsyncReadSamplesExt,
        Cursor,
        combinators::FuncScanner,
        test::SingleSampleStream,
    };

    #[test]
    fn it_zip_two_streams() {
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
