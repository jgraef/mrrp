mod samples;
mod samples_mut;
mod uninit_slice;

use std::{
    marker::PhantomData,
    ops::RangeBounds,
};

pub use crate::buf::{
    samples::Samples,
    samples_mut::SamplesMut,
    uninit_slice::UninitSlice,
};
use crate::io::ReadBuf;

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Tried to read {requested} samples from a buffer with {available} samples remaining.")]
pub struct TryGetError {
    pub requested: usize,
    pub available: usize,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Tried to write {write_length} samples into a buffer with {available} space left.")]
pub struct TryPutError {
    pub write_length: usize,
    pub available: usize,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Tried to advance by {requested} samples a buffer with {available} samples remaining.")]
pub struct TryAdvanceError {
    pub requested: usize,
    pub available: usize,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Index {index} out of bounds 0..{length}")]
pub struct IndexOutOfBounds {
    pub index: usize,
    pub length: usize,
}

pub trait SampleBuf<S> {
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError>;
    fn remaining(&self) -> usize;
    fn chunk(&self) -> &[S];

    #[inline]
    fn advance(&mut self, amount: usize) {
        self.try_advance(amount).unwrap()
    }

    #[inline]
    fn has_remaining(&self) -> bool {
        self.remaining() != 0
    }

    #[inline]
    fn try_get_sample(&mut self) -> Result<S, TryGetError>
    where
        S: Clone,
    {
        let sample = self
            .chunk()
            .first()
            .ok_or_else(|| {
                TryGetError {
                    requested: 1,
                    available: self.remaining(),
                }
            })?
            .clone();
        self.advance(1);
        Ok(sample)
    }

    #[inline]
    fn get_sample(&mut self) -> S
    where
        S: Clone,
    {
        self.try_get_sample().unwrap()
    }

    #[inline]
    fn chain<U>(self, next: U) -> Chain<Self, U, S>
    where
        Self: Sized,
        U: SampleBuf<S>,
    {
        Chain {
            head: self,
            tail: next,
            _phantom: PhantomData,
        }
    }

    #[inline]
    fn take(self, limit: usize) -> Take<Self, S>
    where
        Self: Sized,
    {
        let limit = limit.min(self.remaining());
        Take {
            inner: self,
            limit,
            _phantom: PhantomData,
        }
    }

    fn try_copy_to_samples(&mut self, length: usize) -> Result<Samples<S>, TryGetError>
    where
        S: Clone,
    {
        if self.remaining() < length {
            Err(TryGetError {
                requested: length,
                available: self.remaining(),
            })
        }
        else {
            let mut output = SamplesMut::with_capacity(length);
            output.put((&mut *self).take(length));
            Ok(output.freeze())
        }
    }

    #[inline]
    fn copy_to_samples(&mut self, length: usize) -> Samples<S>
    where
        S: Clone,
    {
        self.try_copy_to_samples(length).unwrap()
    }

    fn try_copy_to_slice(&mut self, mut output: &mut [S]) -> Result<(), TryGetError>
    where
        S: Clone,
    {
        if self.remaining() < output.len() {
            Err(TryGetError {
                requested: output.len(),
                available: self.remaining(),
            })
        }
        else {
            output.put(self.take(output.len()));
            Ok(())
        }
    }

    #[inline]
    fn copy_to_slice(&mut self, output: &mut [S])
    where
        S: Clone,
    {
        self.try_copy_to_slice(output).unwrap()
    }
}

impl<T, S> SampleBuf<S> for &mut T
where
    T: SampleBuf<S> + ?Sized,
{
    #[inline]
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError> {
        (*self).try_advance(amount)
    }

    #[inline]
    fn remaining(&self) -> usize {
        (&**self).remaining()
    }

    #[inline]
    fn chunk(&self) -> &[S] {
        (&**self).chunk()
    }
}

impl<S> SampleBuf<S> for &[S] {
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError> {
        if amount > self.len() {
            Err(TryAdvanceError {
                requested: amount,
                available: self.len(),
            })
        }
        else {
            *self = &(*self)[amount..];
            Ok(())
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk(&self) -> &[S] {
        *self
    }
}

#[derive(Clone, Debug)]
pub struct Chain<H, T, S> {
    head: H,
    tail: T,
    _phantom: PhantomData<fn() -> S>,
}

impl<H, T, S> SampleBuf<S> for Chain<H, T, S>
where
    H: SampleBuf<S>,
    T: SampleBuf<S>,
{
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError> {
        let head_remaining = self.head.remaining();

        if head_remaining == 0 {
            self.tail.try_advance(amount)?;
        }
        else if head_remaining >= amount {
            self.head.advance(head_remaining);
        }
        else {
            self.head.advance(head_remaining);
            self.tail.try_advance(amount - head_remaining)?;
        }

        Ok(())
    }

    fn remaining(&self) -> usize {
        self.head.remaining().saturating_add(self.tail.remaining())
    }

    fn chunk(&self) -> &[S] {
        let head_chunk = self.head.chunk();
        if head_chunk.is_empty() {
            self.tail.chunk()
        }
        else {
            head_chunk
        }
    }
}

#[derive(Clone, Debug)]
pub struct Take<B, S> {
    inner: B,
    limit: usize,
    _phantom: PhantomData<fn() -> S>,
}

impl<B, S> SampleBuf<S> for Take<B, S>
where
    B: SampleBuf<S>,
{
    #[inline]
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError> {
        if amount > self.limit {
            Err(TryAdvanceError {
                requested: amount,
                available: self.limit,
            })
        }
        else {
            self.inner.try_advance(amount)?;
            Ok(())
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.limit
    }

    fn chunk(&self) -> &[S] {
        let mut chunk = self.inner.chunk();
        if chunk.len() > self.limit {
            chunk = &chunk[..self.limit];
        }
        chunk
    }
}

pub trait SampleBufMut<S> {
    unsafe fn advance_mut(&mut self, amount: usize);
    fn remaining_mut(&self) -> usize;
    fn chunk_mut(&mut self) -> &mut UninitSlice<S>;

    #[inline]
    fn has_remaining_mut(&self) -> bool {
        self.remaining_mut() > 0
    }

    fn try_put_sample(&mut self, sample: S) -> Result<(), TryPutError> {
        if self.remaining_mut() == 0 {
            Err(TryPutError {
                write_length: 1,
                available: self.remaining_mut(),
            })
        }
        else {
            let destination_chunk = self.chunk_mut();
            destination_chunk.write_sample(0, sample);
            unsafe {
                self.advance_mut(1);
            }
            Ok(())
        }
    }

    #[inline]
    fn put_sample(&mut self, sample: S) {
        self.try_put_sample(sample).unwrap()
    }

    fn try_put<B>(&mut self, mut source: B) -> Result<(), TryPutError>
    where
        B: SampleBuf<S>,
        S: Clone,
    {
        if self.remaining_mut() < source.remaining() {
            Err(TryPutError {
                write_length: source.remaining(),
                available: self.remaining_mut(),
            })
        }
        else {
            loop {
                let source_chunk = source.chunk();
                let destination_chunk = self.chunk_mut();
                let n = source_chunk.len().min(destination_chunk.len());
                destination_chunk.clone_from_slice(&source_chunk[..n]);
                source.advance(n);
                unsafe {
                    self.advance_mut(n);
                }
            }
        }
    }

    #[inline]
    fn put<B>(&mut self, source: B)
    where
        B: SampleBuf<S>,
        S: Clone,
    {
        self.try_put(source).unwrap();
    }

    #[inline]
    fn with_read_buf<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut ReadBuf<S>) -> R,
    {
        let mut read_buf = ReadBuf::uninit(self.chunk_mut());
        let output = f(&mut read_buf);
        let num_samples_read = read_buf.filled().len();
        unsafe {
            self.advance_mut(num_samples_read);
        }
        output
    }
}

impl<S> SampleBufMut<S> for &mut [S] {
    unsafe fn advance_mut(&mut self, amount: usize) {
        if amount > self.len() {
            panic!(
                "{}",
                TryAdvanceError {
                    requested: amount,
                    available: self.len(),
                }
            );
        }

        let (_, right) = core::mem::take(self).split_at_mut(amount);
        *self = right;
    }

    #[inline]
    fn remaining_mut(&self) -> usize {
        self.len()
    }

    #[inline]
    fn chunk_mut(&mut self) -> &mut UninitSlice<S> {
        UninitSlice::slice_mut_from_init(*self)
    }
}

impl<S> SampleBufMut<S> for Vec<S> {
    unsafe fn advance_mut(&mut self, amount: usize) {
        let new_length = self.len() + amount;
        assert!(new_length <= self.capacity());
        unsafe {
            self.set_len(new_length);
        }
    }

    fn remaining_mut(&self) -> usize {
        usize::MAX.saturating_sub(self.len())
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice<S> {
        self.reserve(1);
        UninitSlice::slice_mut_from_uninit(self.spare_capacity_mut())
    }
}

#[derive(Clone, Debug)]
pub struct IntoIter<B, S> {
    buf: B,
    _phantom: PhantomData<fn() -> S>,
}

impl<B, S> IntoIter<B, S> {
    #[inline]
    pub fn new(buf: B) -> Self {
        Self {
            buf,
            _phantom: PhantomData,
        }
    }
}

impl<B: SampleBuf<S>, S> Iterator for IntoIter<B, S>
where
    S: Clone,
{
    type Item = S;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.buf.try_get_sample().ok()
    }
}

fn slice_bounds(
    range: impl RangeBounds<usize>,
    length: usize,
) -> Result<(usize, usize), IndexOutOfBounds> {
    let (start, end) = crate::util::slice_bounds(range, 0, length);

    if start > length {
        Err(IndexOutOfBounds {
            index: start,
            length,
        })
    }
    else if end > length {
        Err(IndexOutOfBounds { index: end, length })
    }
    else {
        Ok((start, end))
    }
}
