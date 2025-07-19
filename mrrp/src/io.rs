use std::{
    convert::Infallible,
    marker::PhantomData,
    mem::MaybeUninit,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytemuck::Pod;
use pin_project_lite::pin_project;

use crate::buf::{
    SampleBufMut,
    UninitSlice,
};

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
        let max = self.remaining().max(n);
        ReadBuf {
            buffer: &mut self.buffer[self.filled..][..max],
            filled: 0,
            initialized: self.initialized - self.filled,
        }
    }

    #[inline]
    pub fn initialized(&self) -> &[S] {
        unsafe { self.buffer[..self.initialized].assume_init() }
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
        self.buffer[self.filled..][..samples.len()].clone_from_slice(samples);
        self.filled += samples.len();
        self.initialized = self.initialized.max(self.filled);
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
pub trait AsyncReadSamples<S> {
    /// Error that might occur when reading the IQ stream.
    type Error;

    /// Poll the stream to fill a buffer with IQ samples.
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>>;
}

impl<S, T> AsyncReadSamples<S> for &mut T
where
    T: ?Sized + AsyncReadSamples<S> + Unpin,
{
    type Error = <T as AsyncReadSamples<S>>::Error;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut **self).poll_read_samples(cx, buffer)
    }
}

/// Extension trait for [`AsyncReadSamples`] with some useful methods.
pub trait AsyncReadSamplesExt<S>: AsyncReadSamples<S> {
    #[inline]
    fn poll_read_samples_unpin(
        &mut self,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>>
    where
        Self: Unpin,
    {
        Pin::new(self).poll_read_samples(cx, buffer)
    }

    /// Read a single sample
    #[inline]
    fn read_sample(&mut self) -> ReadSample<'_, S, Self>
    where
        Self: Unpin,
    {
        ReadSample {
            inner: self,
            _phantom: PhantomData,
        }
    }

    /// Read IQ samples into a buffer.
    ///
    /// This will call
    /// [`poll_read_samples`][AsyncReadSamples::poll_read_samples] exactly once,
    /// and return the number of bytes read. This is cancellation-safe.
    #[inline]
    fn read_samples<'a>(&'a mut self, buffer: &'a mut [S]) -> ReadSamples<'a, S, Self>
    where
        Self: Unpin,
    {
        ReadSamples {
            inner: self,
            buffer: ReadBuf::new(buffer),
        }
    }

    /// Read IQ samples into a buffer until the buffer is full.
    ///
    /// This might call
    /// [`poll_read_samples`][AsyncReadSamples::poll_read_samples] multiple
    /// times, and thus is not cancellation-safe.
    #[inline]
    fn read_samples_exact<'a>(&'a mut self, buffer: &'a mut [S]) -> ReadSamplesExact<'a, S, Self>
    where
        Self: Unpin,
    {
        ReadSamplesExact {
            inner: self,
            buffer: ReadBuf::new(buffer),
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
        MapErr {
            inner: self,
            map_err: f,
        }
    }

    #[inline]
    fn map<Q, F>(self, f: F, buffer_size: usize) -> Map<Self, S, F>
    where
        F: FnMut(S) -> Q,
        Self: Sized,
    {
        Map {
            inner: self,
            map: f,
            intermediate_buffer: UninitSlice::box_new(buffer_size),
        }
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
    #[inline]
    fn map_in_place<Q, F>(self, f: F) -> MapInPlace<Self, S, F>
    where
        S: Pod,
        Q: Pod,
        F: FnMut(S) -> Q,
        Self: Sized,
    {
        MapInPlace {
            inner: self,
            map: f,
            _phantom: PhantomData,
        }
    }
}

impl<S, R> AsyncReadSamplesExt<S> for R where R: AsyncReadSamples<S> + ?Sized {}

#[derive(Debug)]
pub struct ReadSample<'a, S, R>
where
    R: ?Sized,
{
    inner: &'a mut R,
    _phantom: PhantomData<fn() -> S>,
}

impl<'a, S, R> Future for ReadSample<'a, S, R>
where
    R: AsyncReadSamples<S> + Unpin + ?Sized,
    // todo: remove this bound
    S: Default,
{
    type Output = Result<S, EofError<R::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut buffer = [MaybeUninit::uninit(); 1];
        let mut read_buf = ReadBuf::uninit(UninitSlice::slice_mut_from_uninit(&mut buffer[..]));

        match Pin::new(&mut *self.inner).poll_read_samples(cx, &mut read_buf) {
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
pub struct ReadSamples<'a, S, R>
where
    R: ?Sized,
{
    inner: &'a mut R,
    buffer: ReadBuf<'a, S>,
}

impl<'a, 'b, S, R> Future for ReadSamples<'a, S, R>
where
    R: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Output = Result<usize, R::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;
        Pin::new(&mut *this.inner)
            .poll_read_samples(cx, &mut this.buffer)
            .map_ok(|()| self.buffer.filled().len())
    }
}

/// Future that tries to read an exact amount of samples.
#[derive(Debug)]
pub struct ReadSamplesExact<'a, S, R>
where
    R: ?Sized,
{
    inner: &'a mut R,
    buffer: ReadBuf<'a, S>,
}

impl<'a, 'b, S, T> Future for ReadSamplesExact<'a, S, T>
where
    T: AsyncReadSamples<S> + Unpin + ?Sized,
{
    type Output = Result<(), EofError<T::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while self.buffer.remaining() > 0 {
            let filled_before = self.buffer.filled().len();
            let this = &mut *self;

            match this.inner.poll_read_samples_unpin(cx, &mut this.buffer) {
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

pin_project! {
    /// Stream wrapper that maps the error type.
    #[derive(Clone, Copy, Debug)]
    pub struct MapErr<R, F> {
        #[pin]
        inner: R,
        map_err: F,
    }
}

impl<S, R, E, F> AsyncReadSamples<S> for MapErr<R, F>
where
    R: AsyncReadSamples<S>,
    F: FnMut(R::Error) -> E,
{
    type Error = E;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        this.inner
            .poll_read_samples(cx, buffer)
            .map_err(this.map_err)
    }
}

pin_project! {
    /// Stream wrapper that maps the samples using an intermediate buffer.
    #[derive(Debug)]
    pub struct Map<R, S, F> {
        #[pin]
        inner: R,
        map: F,
        intermediate_buffer: Box<UninitSlice<S>>,
    }
}

impl<S, R, Q, F> AsyncReadSamples<Q> for Map<R, S, F>
where
    R: AsyncReadSamples<S>,
    F: FnMut(S) -> Q,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Q>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        let read_length = this.intermediate_buffer.len().min(buffer.remaining());
        let mut read_buf = ReadBuf::uninit(&mut this.intermediate_buffer[..read_length]);

        match this.inner.poll_read_samples(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let filled = read_buf.filled().len();

                for i in 0..filled {
                    let sample = unsafe { this.intermediate_buffer[i].assume_init_read() };
                    let sample = (this.map)(sample);

                    buffer.unfilled_mut().write_sample(i, sample);
                }

                unsafe {
                    buffer.assume_init(filled);
                    buffer.set_filled(buffer.filled().len() + filled);
                }

                Poll::Ready(Ok(()))
            }
        }
    }
}

pin_project! {
    /// Stream wrapper that maps the error type.
    #[derive(Clone, Copy, Debug)]
    pub struct MapInPlace<R, S, F> {
        #[pin]
        inner: R,
        map: F,
        _phantom: PhantomData<fn(S)>,
    }
}

impl<S, R, Q, F> AsyncReadSamples<Q> for MapInPlace<R, S, F>
where
    S: Pod,
    Q: Pod,
    R: AsyncReadSamples<S>,
    F: FnMut(S) -> Q,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Q>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();
        let num_samples_out = buffer.remaining();
        const MIN_BUFFER: usize = 32;

        if num_samples_out == 0 {
            Poll::Ready(Ok(()))
        }
        else if num_samples_out < MIN_BUFFER {
            // fall back to using stack-allocated intermediate buffer
            // otherwise a caller like read_exact will provide smaller and smaller buffers,
            // until this can't use it as an intermediate buffer anymore.
            //
            // however this is only a problem if the input samples are larger than the
            // output samples. we do it in either case here though.
            //
            // and furthermore MIN_BUFFER should not be constant, as this edge case really
            // depends on the size difference and alignment. so this needs fixing someway.
            let mut intermediate_buffer = [S::zeroed(); MIN_BUFFER];
            let mut read_buf = ReadBuf::new(&mut intermediate_buffer[..num_samples_out]);

            match this.inner.poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let num_samples_read_in = read_buf.filled().len();

                    for i in 0..num_samples_read_in {
                        let sample = (this.map)(intermediate_buffer[i]);
                        buffer.unfilled_mut().write_sample(i, sample);
                    }

                    unsafe {
                        buffer.assume_init(num_samples_read_in);
                        buffer.set_filled(buffer.filled().len() + num_samples_read_in);
                    }

                    Poll::Ready(Ok(()))
                }
            }
        }
        else {
            let buffer_initialized = buffer.initialize_unfilled(|| Q::zeroed());
            let (_, buffer_in, _) = bytemuck::pod_align_to_mut::<Q, S>(buffer_initialized);

            let num_samples_in = buffer_in.len();
            let num_samples = num_samples_out.min(num_samples_in);
            let buffer_in = &mut buffer_in[..num_samples];

            assert!(
                buffer_in.len() > 0,
                "bug: not a single input sample fits into the provided output buffer"
            );

            let mut read_buf = ReadBuf::new(buffer_in);

            match this.inner.poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let num_samples_read_in = read_buf.filled().len();

                    if num_samples_out < num_samples_in {
                        // num_samples_out < num_samples_in
                        // sizeof(sample_out) > sizeof(sample_in)
                        // map reverse
                        for i in (0..num_samples_read_in).rev() {
                            let (_, buffer_in, _) =
                                bytemuck::pod_align_to::<Q, S>(buffer_initialized);
                            let sample = buffer_in[i];
                            buffer_initialized[i] = (this.map)(sample);
                        }
                    }
                    else {
                        // num_samples_out >= num_samples_in
                        // sizeof(sample_out) =< sizeof(sample_in)
                        // map forward
                        for i in 0..num_samples_read_in {
                            let (_, buffer_in, _) =
                                bytemuck::pod_align_to::<Q, S>(buffer_initialized);
                            let sample = buffer_in[i];
                            buffer_initialized[i] = (this.map)(sample);
                        }
                    }

                    buffer.set_filled(buffer.filled().len() + num_samples_read_in);

                    Poll::Ready(Ok(()))
                }
            }
        }
    }
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

pub fn repeat<S>(sample: S) -> Repeat<S> {
    Repeat { sample }
}

#[derive(Clone, Copy, Debug)]
pub struct Cursor<'a, S> {
    pub samples: &'a [S],
    pub position: usize,
}

impl<'a, S> Cursor<'a, S> {
    #[inline(always)]
    pub fn advance(&mut self, amount: usize) {
        self.position += amount;
    }

    #[inline(always)]
    pub fn advance_to_end(&mut self) {
        self.position = self.samples.len();
    }

    #[inline(always)]
    pub fn remaining(&self) -> &[S] {
        &self.samples[self.position..]
    }
}

#[cfg(test)]
mod tests {
    use futures_util::FutureExt;

    use crate::io::{
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
            .map(|sample| i16::from(sample) * -2, 20)
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
            .map_in_place(|_sample| -23i8)
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
            .map_in_place(|_sample| -23i16)
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
            .map_in_place(|_sample| -23i8)
            .read_samples_exact(&mut output)
            .now_or_never()
            .expect("test stream pending")
            .expect("test stream error");
        output.iter().for_each(|sample| {
            assert_eq!(*sample, -23);
        });
    }
}
