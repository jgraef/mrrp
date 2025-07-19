use std::{
    ops::{
        Deref,
        DerefMut,
    },
    sync::Arc,
};

use crate::buf::{
    IntoIter,
    SampleBuf,
    SampleBufMut,
    TryAdvanceError,
    samples::Samples,
    uninit_slice::UninitSlice,
};

// note: for now this can be a simple wrapper around Vec, but in future we might
// do something like the bytes crate does.
#[derive(Clone, Debug)]
pub struct SamplesMut<S> {
    buffer: Vec<S>,
    start: usize,
}

impl<S> SamplesMut<S> {
    #[inline]
    pub fn new() -> Self {
        Vec::new().into()
    }

    #[inline]
    pub fn with_capacity(capacity: usize) -> Self {
        Vec::with_capacity(capacity).into()
    }

    #[inline]
    pub fn from_fn(length: usize, sample: impl FnMut() -> S) -> Self {
        let mut buffer = Vec::with_capacity(length);
        buffer.resize_with(length, sample);
        buffer.into()
    }

    #[inline]
    pub fn freeze(mut self) -> Samples<S> {
        // this could be done in O(1) if we used an Arc<UninitSlice> internally. We can
        // just do get_mut_unchecked on it, since we are the only owner. but we would
        // also have to do the growing ourselves.
        let length = self.len();
        let mut buffer = UninitSlice::<S>::arc_new(length);
        let buffer_mut = unsafe { Arc::get_mut_unchecked(&mut buffer) };
        for (i, sample) in self.buffer.drain(self.start..).enumerate() {
            buffer_mut.write_sample(i, sample);
        }
        unsafe { Samples::from_uninit(buffer, length, 0, length) }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.buffer.len() - self.start
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buffer.capacity() - self.start
    }

    #[inline]
    pub fn clear(&mut self) {
        self.buffer.clear();
    }

    #[inline]
    pub fn truncate(&mut self, length: usize) {
        self.buffer.truncate(self.start + length);
    }

    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.buffer.reserve(additional);
    }

    #[inline]
    pub fn resize_with(&mut self, new_length: usize, sample: impl FnMut() -> S) {
        self.buffer.resize_with(new_length + self.start, sample);
    }

    #[inline]
    pub fn resize(&mut self, new_length: usize, sample: S)
    where
        S: Clone,
    {
        self.resize_with(new_length, || sample.clone());
    }

    #[inline]
    pub fn spare_capacity_mut(&mut self) -> &mut UninitSlice<S> {
        UninitSlice::slice_mut_from_uninit(self.buffer.spare_capacity_mut())
    }

    #[inline]
    pub unsafe fn set_length(&mut self, length: usize) {
        unsafe {
            self.buffer.set_len(self.start + length);
        }
    }

    #[inline]
    pub fn extend_from_slice(&mut self, extend: &[S])
    where
        S: Clone,
    {
        self.buffer.extend_from_slice(extend);
    }

    #[inline]
    fn full_slice(&self) -> &[S] {
        &self.buffer[self.start..]
    }

    #[inline]
    fn full_slice_mut(&mut self) -> &mut [S] {
        &mut self.buffer[self.start..]
    }
}

impl<S> SampleBuf<S> for SamplesMut<S> {
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError> {
        if amount > self.remaining() {
            Err(TryAdvanceError {
                requested: amount,
                available: self.remaining(),
            })
        }
        else {
            self.start += amount;
            Ok(())
        }
    }

    fn remaining(&self) -> usize {
        self.buffer.len() - self.start
    }

    fn chunk(&self) -> &[S] {
        self.full_slice()
    }
}

impl<S> SampleBufMut<S> for SamplesMut<S> {
    #[inline]
    unsafe fn advance_mut(&mut self, amount: usize) {
        unsafe {
            self.set_length(self.buffer.len() + amount);
        }
    }

    #[inline]
    fn remaining_mut(&self) -> usize {
        usize::MAX - self.buffer.len()
    }

    #[inline]
    fn chunk_mut(&mut self) -> &mut UninitSlice<S> {
        self.spare_capacity_mut()
    }
}

impl<S> Default for SamplesMut<S> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<S> From<Vec<S>> for SamplesMut<S> {
    #[inline]
    fn from(value: Vec<S>) -> Self {
        Self {
            buffer: value,
            start: 0,
        }
    }
}

impl<S> From<Box<[S]>> for SamplesMut<S> {
    #[inline]
    fn from(value: Box<[S]>) -> Self {
        Vec::from(value).into()
    }
}

impl<S> FromIterator<S> for SamplesMut<S> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = S>>(iter: T) -> Self {
        Self {
            buffer: iter.into_iter().collect(),
            start: 0,
        }
    }
}

impl<S> Extend<S> for SamplesMut<S> {
    fn extend<T: IntoIterator<Item = S>>(&mut self, iter: T) {
        self.buffer.extend(iter);
    }
}

impl<S> AsRef<[S]> for SamplesMut<S> {
    fn as_ref(&self) -> &[S] {
        self.full_slice()
    }
}

impl<S> AsMut<[S]> for SamplesMut<S> {
    fn as_mut(&mut self) -> &mut [S] {
        self.full_slice_mut()
    }
}

impl<S> Deref for SamplesMut<S> {
    type Target = [S];

    fn deref(&self) -> &Self::Target {
        self.full_slice()
    }
}

impl<S> DerefMut for SamplesMut<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.full_slice_mut()
    }
}

impl<S> IntoIterator for SamplesMut<S>
where
    S: Clone,
{
    type Item = S;
    type IntoIter = IntoIter<Self, S>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter::new(self)
    }
}

impl<'a, S> IntoIterator for &'a SamplesMut<S>
where
    S: Clone,
{
    type Item = S;
    type IntoIter = IntoIter<&'a [S], S>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter::new(self.full_slice())
    }
}
