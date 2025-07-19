use std::{
    ops::{
        Deref,
        RangeBounds,
    },
    sync::Arc,
};

use crate::buf::{
    IntoIter,
    SampleBuf,
    TryAdvanceError,
    slice_bounds,
    uninit_slice::UninitSlice,
};

#[derive(Clone, Debug)]
pub struct Samples<S> {
    buffer: Arc<UninitSlice<S>>,
    initialized: usize,
    start: usize,
    length: usize,
}

impl<S> Samples<S> {
    #[inline]
    pub fn new() -> Self {
        Self {
            buffer: UninitSlice::arc_new(0),
            initialized: 0,
            start: 0,
            length: 0,
        }
    }

    pub fn from_fn(length: usize, mut samples: impl FnMut() -> S) -> Self {
        let mut buffer = UninitSlice::arc_new(length);
        let buffer_mut = unsafe { Arc::get_mut_unchecked(&mut buffer) };
        for i in 0..length {
            buffer_mut.write_sample(i, samples());
        }
        Self {
            buffer,
            initialized: length,
            start: 0,
            length,
        }
    }

    #[inline]
    pub unsafe fn from_uninit(
        buffer: Arc<UninitSlice<S>>,
        initialized: usize,
        start: usize,
        length: usize,
    ) -> Self {
        Self {
            buffer,
            initialized,
            start,
            length,
        }
    }

    #[inline]
    pub fn from_init(buffer: Arc<[S]>, start: usize, length: usize) -> Self {
        let initialized = buffer.len();
        Self {
            buffer: UninitSlice::arc_from_init(buffer),
            initialized,
            start,
            length,
        }
    }

    #[inline]
    pub fn clear(&mut self) {
        self.truncate(0);
    }

    pub fn clone_from_slice(samples: &[S]) -> Self
    where
        S: Clone,
    {
        let mut buffer = UninitSlice::<S>::arc_new(samples.len());
        let buffer_mut = unsafe { Arc::get_mut_unchecked(&mut buffer) };
        buffer_mut.clone_from_slice(samples);

        Self {
            buffer: buffer.into(),
            initialized: samples.len(),
            start: 0,
            length: samples.len(),
        }
    }

    #[inline]
    pub fn truncate(&mut self, length: usize) {
        if length < self.length {
            if length == 0 {
                *self = Self::new();
            }

            self.length = length;
        }
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.length
    }

    #[inline]
    pub fn is_unique(&self) -> bool {
        // note: this only works if we never use weak references and don't expose the
        // underlying Arc, so nobody can obtain any weak references.
        // if there are weak references, this function is useless anyway, since between
        // calling this and acting on the result another use could upgrade a weak ref.
        // yes we could check if there are no weak references, but what are you supposed
        // to do when there are some? e.g. if you want to do cow: do you clone and
        // invalidate the weak reference, or not?
        Arc::strong_count(&self.buffer) == 1
    }

    #[inline]
    fn full_slice(&self) -> &[S] {
        assert!(self.start + self.length <= self.initialized);
        unsafe { self.buffer[self.start..][..self.length].assume_init() }
    }

    #[inline]
    pub fn slice(&self, range: impl RangeBounds<usize>) -> Self {
        let (start, end) = slice_bounds(range, self.length).unwrap();
        Self {
            buffer: self.buffer.clone(),
            initialized: self.initialized,
            start,
            length: end - start,
        }
    }
}

impl<S> Drop for Samples<S> {
    fn drop(&mut self) {
        if let Some(buffer) = Arc::get_mut(&mut self.buffer) {
            unsafe {
                buffer[..self.initialized].assume_init_drop();
            }
        }
    }
}

impl<S> SampleBuf<S> for Samples<S> {
    #[inline]
    fn try_advance(&mut self, amount: usize) -> Result<(), TryAdvanceError> {
        if amount > self.length {
            Err(TryAdvanceError {
                requested: amount,
                available: self.length,
            })
        }
        else {
            self.start += amount;
            self.length -= amount;
            Ok(())
        }
    }

    #[inline]
    fn remaining(&self) -> usize {
        self.length
    }

    #[inline]
    fn chunk(&self) -> &[S] {
        self.full_slice()
    }
}

impl<S> Default for Samples<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> From<Arc<[S]>> for Samples<S> {
    #[inline]
    fn from(value: Arc<[S]>) -> Self {
        let length = value.len();
        Self::from_init(value, 0, length)
    }
}

impl<S> From<Box<[S]>> for Samples<S> {
    #[inline]
    fn from(value: Box<[S]>) -> Self {
        Arc::<[S]>::from(value).into()
    }
}

impl<S> From<Vec<S>> for Samples<S> {
    #[inline]
    fn from(value: Vec<S>) -> Self {
        Arc::<[S]>::from(value).into()
    }
}

impl<S> FromIterator<S> for Samples<S> {
    #[inline]
    fn from_iter<T: IntoIterator<Item = S>>(iter: T) -> Self {
        iter.into_iter().collect::<Arc<[S]>>().into()
    }
}

impl<S> AsRef<[S]> for Samples<S> {
    #[inline]
    fn as_ref(&self) -> &[S] {
        self.full_slice()
    }
}

impl<S> Deref for Samples<S> {
    type Target = [S];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.full_slice()
    }
}

impl<S> IntoIterator for Samples<S>
where
    S: Clone,
{
    type Item = S;
    type IntoIter = IntoIter<Samples<S>, S>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        IntoIter::new(self)
    }
}

impl<'a, S> IntoIterator for &'a Samples<S>
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
