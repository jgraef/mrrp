use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use crate::{
    buf::{
        SampleBufMut,
        UninitSlice,
    },
    signal::StreamLength,
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
pub trait AsyncReadSamples: StreamLength {
    type Sample;
    /// Error that might occur when reading the IQ stream.
    type Error;

    /// Poll the stream to fill a buffer with IQ samples.
    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>>;
}

impl<R> AsyncReadSamples for &mut R
where
    R: AsyncReadSamples + Unpin + ?Sized,
{
    type Sample = <R as AsyncReadSamples>::Sample;
    type Error = <R as AsyncReadSamples>::Error;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut **self).poll_read_samples(cx, buffer)
    }
}
