use std::{
    convert::Infallible,
    fmt::Debug,
    marker::PhantomData,
    mem::MaybeUninit,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use bytemuck::Pod;
use tracing::Span;

use crate::{
    buf::{
        SampleBuf,
        SampleBufMut,
        UninitSlice,
    },
    filter::resampling::{
        Decimate,
        Interpolate,
    },
    io::{
        AsyncWriteSamples,
        FiniteStream,
        Forward,
        GetSampleRate,
        Remaining,
        StreamLength,
        combinators::{
            Buffered,
            Chained,
            Converted,
            Inspect,
            InspectWith,
            Limited,
            Map,
            MapErr,
            MapInPlace,
            MapInPlacePod,
            Multiplied,
            Repeated,
            ScanInPlaceWith,
            ScanWith,
            Scanner,
            Summed,
            Throttled,
            WithSampleRate,
            WithSpan,
            ZipWith,
        },
    },
    sample::{
        FromSample,
        Sample,
    },
};

// todo: We really must make this S: Copy. Lots of places assume this and it's a
// hassle otherwise (e.g. when writing to unfilled_mut()). otherwise removing
// the filled..initialized portion of the buffer would make things a lot easier.
#[derive(Debug)]
pub struct ReadBuf<'a, S> {
    buffer: &'a mut UninitSlice<S>,
    filled: usize,
    initialized: usize,
}

impl<'a, S> ReadBuf<'a, S> {
    #[inline]
    pub fn new(buffer: &'a mut [S]) -> Self {
        let length = buffer.len();
        Self {
            buffer: UninitSlice::slice_mut_from_init(buffer),
            filled: 0,
            initialized: length,
        }
    }

    #[inline]
    pub fn uninit(buffer: &'a mut UninitSlice<S>) -> Self {
        Self {
            buffer,
            filled: 0,
            initialized: 0,
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    #[inline]
    pub fn filled(&self) -> &[S] {
        &self.initialized()[..self.filled]
    }

    #[inline]
    pub fn filled_mut(&mut self) -> &mut [S] {
        let filled = self.filled;
        &mut self.initialized_mut()[..filled]
    }

    #[inline]
    pub fn take(&mut self, n: usize) -> ReadBuf<'_, S> {
        let max = self.remaining().min(n);
        ReadBuf {
            buffer: &mut self.buffer[self.filled..][..max],
            filled: 0,
            initialized: self.initialized - self.filled,
        }
    }

    #[inline]
    pub fn initialized(&self) -> &[S] {
        unsafe { self.buffer[..self.initialized].assume_init_ref() }
    }

    #[inline]
    pub fn initialized_mut(&mut self) -> &mut [S] {
        unsafe { self.buffer[..self.initialized].assume_init_mut() }
    }

    #[inline]
    pub fn inner_mut(&mut self) -> &mut UninitSlice<S> {
        self.buffer
    }

    #[inline]
    pub fn unfilled_mut(&mut self) -> &mut UninitSlice<S> {
        &mut self.buffer[self.filled..]
    }

    #[inline]
    pub fn initialize_unfilled(&mut self, init: impl FnMut() -> S) -> &mut [S] {
        self.initialize_unfilled_to(self.remaining(), init)
    }

    #[inline]
    pub fn initialize_unfilled_to(&mut self, n: usize, init: impl FnMut() -> S) -> &mut [S] {
        let initialize_to = self.filled + n;
        self.buffer[self.initialized..initialize_to].fill_with(init);
        unsafe { self.buffer[self.filled..initialize_to].assume_init_mut() }
    }

    #[inline]
    pub fn fill_with(&mut self, mut fill: impl FnMut() -> S) {
        unsafe {
            self.buffer[self.filled..self.initialized].assume_init_drop();
        }

        for i in self.filled..self.buffer.len() {
            self.buffer.write_sample(i, fill());
        }

        self.filled = self.buffer.len();
        self.initialized = self.buffer.len();
    }

    #[inline]
    pub fn remaining(&self) -> usize {
        self.buffer.len() - self.filled
    }

    #[inline]
    pub fn set_filled(&mut self, filled: usize) {
        assert!(filled <= self.initialized);
        self.filled = filled;
    }

    #[inline]
    pub unsafe fn assume_init(&mut self, n: usize) {
        self.initialized = self.initialized.max(self.filled + n);
    }

    #[inline]
    pub fn put_slice(&mut self, samples: &[S])
    where
        S: Clone,
    {
        assert!(samples.len() + self.filled <= self.buffer.len());

        unsafe {
            self.buffer[self.filled..(self.filled + samples.len()).min(self.initialized)]
                .assume_init_drop();
        }

        self.buffer[self.filled..][..samples.len()].clone_from_slice(samples);
        self.filled += samples.len();
        self.initialized = self.initialized.max(self.filled);
    }

    #[inline]
    pub unsafe fn drop_unfilled_initialized(&mut self) {
        unsafe {
            self.buffer[self.filled..self.initialized].assume_init_drop();
        }
        self.initialized = self.filled;
    }
}

impl<'a, S> SampleBufMut<S> for ReadBuf<'a, S> {
    #[inline]
    unsafe fn advance_mut(&mut self, amount: usize) {
        self.filled += amount;
        self.initialized = self.initialized.max(self.filled);
    }

    #[inline]
    fn remaining_mut(&self) -> usize {
        self.remaining()
    }

    #[inline]
    fn chunk_mut(&mut self) -> &mut UninitSlice<S> {
        self.unfilled_mut()
    }
}

/// Trait for async reading of samples.
///
/// This works pretty much like futures [`AsyncRead`][1],
/// except it works with arbitrary sample types instead of single bytes.
///
/// [1]: https://docs.rs/futures/latest/futures/io/trait.AsyncRead.html
pub trait AsyncReadSamples<S>: StreamLength {
    /// Error that might occur when reading the IQ stream.
    type Error: std::error::Error;

    /// Poll the stream to fill a buffer with IQ samples.
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>>;
}

impl<R, S> AsyncReadSamples<S> for &mut R
where
    R: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Error = <R as AsyncReadSamples<S>>::Error;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut **self).poll_read_samples(cx, buffer)
    }
}

pub trait IntoReadSamples<S> {
    type ReadSamples: AsyncReadSamples<S>;

    fn into_read_samples(self) -> Self::ReadSamples;
}

/// Extension trait for [`AsyncReadSamples`] with some useful methods.
pub trait AsyncReadSamplesExt<S>: AsyncReadSamples<S> {
    #[inline]
    fn poll_read_into<B: SampleBufMut<S>>(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buffer: B,
    ) -> Poll<Result<(), Self::Error>> {
        buffer.with_read_buf(|read_buf| self.poll_read_samples(cx, read_buf))
    }

    /// Read a single sample
    #[inline]
    fn read_sample(&mut self) -> ReadSample<'_, Self, S>
    where
        Self: Unpin,
    {
        ReadSample {
            read_samples: self,
            _phantom: PhantomData,
        }
    }

    /// Read IQ samples into a buffer.
    ///
    /// This will call
    /// [`poll_read_samples`][AsyncReadSamples::poll_read_samples] exactly once,
    /// and return the number of bytes read. This is cancellation-safe.
    #[inline]
    fn read_samples<'a>(&'a mut self, buffer: &'a mut [S]) -> ReadSamples<'a, Self, S>
    where
        Self: Unpin,
    {
        ReadSamples {
            read_samples: self,
            buffer: ReadBuf::new(buffer),
        }
    }

    /// Read IQ samples into a buffer until the buffer is full.
    ///
    /// This might call
    /// [`poll_read_samples`][AsyncReadSamples::poll_read_samples] multiple
    /// times, and thus is not cancellation-safe.
    #[inline]
    fn read_samples_exact<'a>(&'a mut self, buffer: &'a mut [S]) -> ReadSamplesExact<'a, Self, S>
    where
        Self: Unpin,
    {
        ReadSamplesExact {
            read_samples: self,
            buffer: ReadBuf::new(buffer),
        }
    }

    #[inline]
    fn read_to_end<'a>(&'a mut self, buffer: &'a mut Vec<S>) -> ReadToEnd<'a, Self, S>
    where
        Self: Unpin + FiniteStream,
    {
        ReadToEnd {
            read_samples: self,
            buffer,
        }
    }

    /// Maps any errors returned by the underlying stream with the provided
    /// closure.
    #[inline]
    fn map_err<E, F>(self, f: F) -> MapErr<Self, F>
    where
        F: FnMut(Self::Error) -> E,
        Self: Sized,
    {
        MapErr::new(self, f)
    }

    #[inline]
    fn scan_with<Sc>(self, scanner: Sc) -> ScanWith<Self, S, Sc>
    where
        Sc: Scanner<S>,
        Self: Sized,
    {
        ScanWith::new(self, scanner)
    }

    #[inline]
    fn scan_in_place_with<Sc>(self, scanner: Sc) -> ScanInPlaceWith<Self, Sc>
    where
        Sc: Scanner<S, Output = S>,
        Self: Sized,
    {
        ScanInPlaceWith::new(self, scanner)
    }

    #[inline]
    fn map<Q, F>(self, f: F) -> Map<Self, S, F>
    where
        F: FnMut(S) -> Q,
        Self: Sized,
    {
        Map::new(self, f)
    }

    #[inline]
    fn map_in_place<F>(self, f: F) -> MapInPlace<Self, F>
    where
        F: FnMut(S) -> S,
        Self: Sized,
    {
        MapInPlace::new(self, f)
    }

    /// Reads a [`AsyncReadSamples<S>`][AsyncReadSamples] and maps it with the
    /// provided function using the destination buffer.
    ///
    /// # Experimental!
    ///
    /// This uses bytemuck magic to cast the destination buffer to accept `S`
    /// samples and then maps in-place. It is very likely that this still
    /// contains bugs. If you encounter weird behavior, please consider
    /// writing a test for it. And you call always switch to using
    /// [`map`][Self::map] instead.
    ///
    /// # TODO
    ///
    /// - scan_in_place_pod_with which then can be used to implement this
    #[inline]
    fn map_in_place_pod<Q, F>(self, f: F) -> MapInPlacePod<Self, S, F>
    where
        S: Pod,
        Q: Pod,
        F: FnMut(S) -> Q,
        Self: Sized,
    {
        MapInPlacePod::new(self, f)
    }

    #[inline]
    fn inspect_with<I>(self, inspector: I) -> InspectWith<Self, I>
    where
        Self: Sized,
    {
        InspectWith::new(self, inspector)
    }

    #[inline]
    fn inspect<F>(self, f: F) -> Inspect<Self, F>
    where
        F: FnMut(&[S]),
        Self: Sized,
    {
        Inspect::new(self, f)
    }

    #[inline]
    fn buffered(self, buffer_size: usize) -> Buffered<Self, S>
    where
        Self: Sized,
    {
        Buffered::new(self, buffer_size)
    }

    #[inline]
    fn forward<W>(self, sink: W, buffer_size: usize) -> Forward<Self, W, S>
    where
        Self: Sized,
        W: AsyncWriteSamples<S>,
    {
        Forward::new(self, sink, buffer_size)
    }

    #[inline]
    fn with_span(self, span: Span) -> WithSpan<Self>
    where
        Self: Sized,
    {
        WithSpan::new(self, span)
    }

    #[inline]
    fn decimate(self, factor: usize) -> Decimate<Self>
    where
        Self: Sized,
    {
        Decimate::new(self, factor)
    }

    #[inline]
    fn decimate_to(self, target_sample_rate: f32) -> Decimate<Self>
    where
        Self: Sized + GetSampleRate,
    {
        let sample_rate = self.sample_rate();
        self.decimate((sample_rate / target_sample_rate).round() as usize)
    }

    #[inline]
    fn interpolate(self, factor: usize) -> Interpolate<Self>
    where
        Self: Sized,
    {
        Interpolate::new(self, factor)
    }

    #[inline]
    fn interpolate_to(self, target_sample_rate: f32) -> Interpolate<Self>
    where
        Self: Sized + GetSampleRate,
    {
        let sample_rate = self.sample_rate();
        self.interpolate((target_sample_rate / sample_rate).round() as usize)
    }

    #[inline]
    fn throttle(self, sample_duration: Duration) -> Throttled<Self>
    where
        Self: Sized,
    {
        Throttled::new(self, sample_duration)
    }

    #[inline]
    fn throttle_to_sample_rate(self) -> Throttled<Self>
    where
        Self: Sized + GetSampleRate,
    {
        let sample_duration = Duration::from_secs_f32(1.0 / self.sample_rate());
        self.throttle(sample_duration)
    }

    #[inline]
    fn with_sample_rate(self, sample_rate: f32) -> WithSampleRate<Self>
    where
        Self: Sized,
    {
        WithSampleRate::new(self, sample_rate)
    }

    #[inline]
    fn convert<Q>(self) -> Converted<Self, S, Q>
    where
        Self: Sized,
        Q: FromSample<S>,
    {
        Converted::new(self)
    }

    #[inline]
    fn chain<T>(self, other: T) -> Chained<Self, T>
    where
        Self: Sized,
        T: Sized + AsyncReadSamples<S>,
    {
        Chained::new(self, other)
    }

    #[inline]
    fn limit(self, num_samples: usize) -> Limited<Self>
    where
        Self: Sized,
    {
        Limited::new(self, num_samples)
    }

    #[inline]
    fn limit_by_time(self, time: f32) -> Limited<Self>
    where
        Self: Sized + GetSampleRate,
    {
        let num_samples = (time * self.sample_rate()) as usize;
        self.limit(num_samples)
    }

    #[inline]
    fn zip_with<R, T, Sc>(self, other: R, scanner: Sc) -> ZipWith<Self, R, S, T, Sc>
    where
        Self: Sized,
        R: AsyncReadSamples<T> + Sized,
        Sc: Scanner<(S, T)>,
    {
        ZipWith::new(self, other, scanner)
    }

    /// Repeats a stream indefinitely.
    ///
    /// Refer to [`Repeated`] about memory usage.
    ///
    /// In order to avoid memory exhaustion, this is only implemented for finite
    /// streams. You can work around this by creating the [`Repeated`] yourself
    /// though.
    #[inline]
    fn repeat(self) -> Repeated<Self, S>
    where
        Self: Sized + FiniteStream,
    {
        Repeated::new(self)
    }

    #[inline]
    fn add<R, T>(self, other: R) -> Summed<Self, R, S, T>
    where
        Self: Sized,
        R: AsyncReadSamples<R> + Sized,
    {
        Summed::new(self, other)
    }

    #[inline]
    fn mul<R, T>(self, other: R) -> Multiplied<Self, R, S, T>
    where
        Self: Sized,
        R: AsyncReadSamples<R> + Sized,
    {
        Multiplied::new(self, other)
    }
}

impl<R, S> AsyncReadSamplesExt<S> for R where R: AsyncReadSamples<S> + ?Sized {}

#[derive(Debug)]
#[must_use]
pub struct ReadSample<'a, R, S>
where
    R: ?Sized,
{
    read_samples: &'a mut R,
    _phantom: PhantomData<fn() -> S>,
}

impl<'a, R, S> Future for ReadSample<'a, R, S>
where
    R: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Output = Result<S, EofError<R::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut buffer = [MaybeUninit::uninit(); 1];
        let mut read_buf = ReadBuf::uninit(UninitSlice::slice_mut_from_uninit(&mut buffer[..]));

        match Pin::new(&mut *self.read_samples).poll_read_samples(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error.into())),
            Poll::Ready(Ok(())) => {
                if read_buf.filled().len() == 0 {
                    Poll::Ready(Err(EofError::Eof {
                        num_samples_read: 0,
                    }))
                }
                else {
                    let [sample] = buffer;
                    let sample = unsafe {
                        // SAFETY: the buffer has been filled. since our buffer is only 1 sample
                        // wide, this has to have been filled.
                        sample.assume_init()
                    };
                    Poll::Ready(Ok(sample))
                }
            }
        }
    }
}

/// Future that reads samples into a buffer.
#[derive(Debug)]
#[must_use]
pub struct ReadSamples<'a, R, S>
where
    R: ?Sized,
{
    read_samples: &'a mut R,
    buffer: ReadBuf<'a, S>,
}

impl<'a, 'b, R, S> Future for ReadSamples<'a, R, S>
where
    R: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Output = Result<usize, R::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut this.read_samples)
            .poll_read_samples(cx, &mut this.buffer)
            .map_ok(|()| self.buffer.filled().len())
    }
}

/// Future that tries to read an exact amount of samples.
#[derive(Debug)]
#[must_use]
pub struct ReadSamplesExact<'a, R, S>
where
    R: ?Sized,
{
    read_samples: &'a mut R,
    buffer: ReadBuf<'a, S>,
}

impl<'a, 'b, R, S> Future for ReadSamplesExact<'a, R, S>
where
    R: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Output = Result<(), EofError<R::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while self.buffer.remaining() > 0 {
            let filled_before = self.buffer.filled().len();
            let this = &mut *self;

            match Pin::new(&mut this.read_samples).poll_read_samples(cx, &mut this.buffer) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(Err(EofError::Other(error)));
                }
                Poll::Ready(Ok(())) => {
                    let filled_after = self.buffer.filled().len();

                    if filled_before == filled_after {
                        break;
                    }
                }
            }
        }

        if self.buffer.remaining() == 0 {
            Poll::Ready(Ok(()))
        }
        else {
            Poll::Ready(Err(EofError::Eof {
                num_samples_read: self.buffer.filled().len(),
            }))
        }
    }
}

#[derive(Debug)]
#[must_use]
pub struct ReadToEnd<'a, R, S>
where
    R: ?Sized,
{
    read_samples: &'a mut R,
    buffer: &'a mut Vec<S>,
}

impl<'a, 'b, R, S> Future for ReadToEnd<'a, R, S>
where
    R: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Output = Result<(), R::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let this = &mut *self;
            let filled_before = this.buffer.len();

            let size_hint = this.read_samples.size_hint();
            if let Some(upper_bound) = size_hint.upper_bound {
                this.buffer.reserve_exact(upper_bound);
            }
            else {
                this.buffer.reserve(size_hint.lower_bound);
            }

            match this.buffer.with_read_buf(|read_buf| {
                Pin::new(&mut this.read_samples).poll_read_samples(cx, read_buf)
            }) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(Err(error));
                }
                Poll::Ready(Ok(())) => {
                    if this.buffer.len() == filled_before {
                        return Poll::Ready(Ok(()));
                    }
                }
            }
        }
    }
}

/// Error returned by
/// [`read_samples_exact`][AsyncReadSamplesExt::read_samples_exact]
#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum EofError<E> {
    /// The stream ended before the buffer could be filled completely.
    #[error("EOF after {num_samples_read} samples")]
    Eof { num_samples_read: usize },

    /// The underlying stream produced an error.
    #[error("{0}")]
    Other(#[from] E),
}

#[derive(Clone, Copy, Debug)]
pub struct Repeat<S> {
    pub sample: S,
}

impl<S> AsyncReadSamples<S> for Repeat<S>
where
    S: Clone,
{
    type Error = Infallible;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        buffer.fill_with(|| self.sample.clone());
        Poll::Ready(Ok(()))
    }
}

impl<S> StreamLength for Repeat<S> {
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Infinite
    }
}

#[inline]
pub fn repeat<S>(sample: S) -> Repeat<S> {
    Repeat { sample }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NullSource;

impl<S> AsyncReadSamples<S> for NullSource {
    type Error = Infallible;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        Poll::Pending
    }
}

impl StreamLength for NullSource {
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Finite { num_samples: 0 }
    }
}

#[inline]
pub fn null_source() -> NullSource {
    NullSource
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Silence<S> {
    _phantom: PhantomData<fn() -> S>,
}

impl<S> AsyncReadSamples<S> for Silence<S>
where
    S: Sample,
{
    type Error = Infallible;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        buffer.fill_with(|| S::EQUILIBRIUM);
        Poll::Ready(Ok(()))
    }
}

impl<S> StreamLength for Silence<S> {
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Infinite
    }
}

#[inline]
pub fn silence<S>() -> Silence<S>
where
    S: Sample,
{
    Silence {
        _phantom: PhantomData,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Cursor<B, S> {
    buffer: B,
    position: usize,
    _phantom: PhantomData<fn() -> S>,
}

impl<B, S> Cursor<B, S>
where
    B: AsRef<[S]>,
{
    pub fn new(buffer: B) -> Self {
        Self {
            buffer,
            position: 0,
            _phantom: PhantomData,
        }
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn set_position(&mut self, position: usize) {
        assert!(position <= self.buffer.as_ref().len());
    }

    pub fn data(&self) -> &B {
        &self.buffer
    }

    pub fn data_mut(&mut self) -> &mut B {
        &mut self.buffer
    }
}

impl<B, S> AsyncReadSamples<S> for Cursor<B, S>
where
    B: AsRef<[S]> + Unpin,
    S: Clone,
{
    type Error = Infallible;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let chunk = &self.buffer.as_ref()[self.position..];
        let n = buffer.remaining_mut().min(chunk.len());
        buffer.put_slice(&chunk[..n]);
        self.position += n;

        Poll::Ready(Ok(()))
    }
}

impl<B, S> StreamLength for Cursor<B, S>
where
    B: AsRef<[S]>,
{
    fn remaining(&self) -> Remaining {
        Remaining::Finite {
            num_samples: self.buffer.as_ref().len() - self.position,
        }
    }
}

impl<B, S> FiniteStream for Cursor<B, S> {}

#[derive(Clone, Copy, Debug)]
pub struct BufSource<B, S> {
    buffer: B,
    _phantom: PhantomData<fn() -> S>,
}

impl<B, S> BufSource<B, S>
where
    B: SampleBuf<S>,
{
    pub fn new(buffer: B) -> Self {
        Self {
            buffer,
            _phantom: PhantomData,
        }
    }
}

impl<B, S> AsyncReadSamples<S> for BufSource<B, S>
where
    B: SampleBuf<S> + Unpin,
    S: Clone,
{
    type Error = Infallible;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let chunk = self.buffer.chunk();
        let n = buffer.remaining_mut().min(chunk.len());
        buffer.put_slice(&chunk[..n]);
        self.buffer.advance(n);

        Poll::Ready(Ok(()))
    }
}

impl<B, S> StreamLength for BufSource<B, S>
where
    B: SampleBuf<S>,
{
    fn remaining(&self) -> Remaining {
        Remaining::Finite {
            num_samples: self.buffer.remaining(),
        }
    }
}

impl<B, S> FiniteStream for BufSource<B, S> {}

/// Helper to check if a value implements [`AsyncReadSamples`]
pub fn assert_async_read<T, S>(_: &T)
where
    T: AsyncReadSamples<S>,
{
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use crate::io::read::{
        AsyncReadSamplesExt,
        repeat,
    };

    #[test]
    fn repeat_outputs_repeated_samples() {
        let mut input = repeat(12u8);
        let mut output = [0u8; 100];
        input
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        output.iter().for_each(|sample| {
            assert_eq!(*sample, 12);
        });
    }

    #[test]
    fn it_maps() {
        let input = repeat(12u8);
        let mut output = [0i16; 100];
        input
            .map(|sample| i16::from(sample) * -2)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        output.iter().for_each(|sample| {
            assert_eq!(*sample, -24);
        });
    }

    #[test]
    fn it_maps_in_place_with_same_sample_size() {
        let input = repeat(12u8);
        let mut output = [0; 100];
        input
            .map_in_place_pod(|_sample| -23i8)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        output.iter().for_each(|sample| {
            assert_eq!(*sample, -23);
        });
    }

    #[test]
    fn it_maps_in_place_with_smaller_input_samples() {
        let input = repeat(12u8);
        let mut output = [0; 100];
        input
            .map_in_place_pod(|_sample| -23i16)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        output.iter().for_each(|sample| {
            assert_eq!(*sample, -23);
        });
    }

    #[test]
    fn it_maps_in_place_with_larger_input_samples() {
        let input = repeat(12u16);
        let mut output = [0; 100];
        input
            .map_in_place_pod(|_sample| -23i8)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        output.iter().for_each(|sample| {
            assert_eq!(*sample, -23);
        });
    }
}
