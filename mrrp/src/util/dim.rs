use std::{
    collections::VecDeque,
    mem::MaybeUninit,
    ops::{
        Deref,
        DerefMut,
    },
};

use arraydeque::ArrayDeque;
use arrayvec::ArrayVec;

pub trait Dim: Copy + Sized {
    /// Array with fixed size of the dimension
    type Array<T>: ArrayLike<Self, T>;

    /// Vec that is bounded in size by the dimension
    type BoundedVec<T>: VecLike<Self, T>;

    /// Deque that is bounded in size by the dimension
    type BoundedDeque<T>: DequeLike<Self, T>;

    fn dim(&self) -> usize;
}

#[derive(Clone, Copy, Debug)]
pub struct Const<const DIM: usize>;

impl<const DIM: usize> Dim for Const<DIM> {
    type Array<T> = [T; DIM];
    type BoundedVec<T> = ArrayVec<T, DIM>;
    type BoundedDeque<T> = BoundedConstDeque<DIM, T>;

    fn dim(&self) -> usize {
        DIM
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Dyn(pub usize);

impl Dim for Dyn {
    type Array<T> = DynArray<T>;
    type BoundedVec<T> = BoundedDynVec<T>;
    type BoundedDeque<T> = BoundedDynDeque<T>;

    fn dim(&self) -> usize {
        todo!()
    }
}

pub trait ArrayLike<D: Dim, T> {
    fn dim(&self) -> D;

    fn from_fn(dim: D, f: impl FnMut() -> T) -> Self;

    fn as_slice(&self) -> &[T];

    fn as_slice_mut(&mut self) -> &mut [T];

    fn len(&self) -> usize {
        self.dim().dim()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_full(&self) -> bool {
        true
    }
}

pub trait VecLike<D: Dim, T> {
    fn dim(&self) -> D;

    fn new(dim: D) -> Self;

    fn len(&self) -> usize;

    fn capacity(&self) -> usize {
        self.dim().dim()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }
}

pub trait DequeLike<D: Dim, T> {
    fn dim(&self) -> D;

    fn new(dim: D) -> Self;

    fn len(&self) -> usize;

    fn capacity(&self) -> usize {
        self.dim().dim()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn is_full(&self) -> bool {
        self.len() == self.capacity()
    }

    fn push_front(&mut self, value: T) -> Option<T>;

    fn push_back(&mut self, value: T) -> Option<T>;

    fn get(&mut self, index: usize) -> Option<&T>;
}

impl<const DIM: usize, T> ArrayLike<Const<DIM>, T> for [T; DIM] {
    fn dim(&self) -> Const<DIM> {
        Const::<DIM>
    }

    fn from_fn(_dim: Const<DIM>, mut f: impl FnMut() -> T) -> Self {
        let mut vec = ArrayVec::<T, DIM>::new();
        for _ in 0..DIM {
            vec.push(f());
        }
        vec.into_inner().unwrap_or_else(|_| unreachable!())
    }

    #[inline]
    fn as_slice(&self) -> &[T] {
        &self[..]
    }

    #[inline]
    fn as_slice_mut(&mut self) -> &mut [T] {
        &mut self[..]
    }
}

impl<const DIM: usize, T> VecLike<Const<DIM>, T> for ArrayVec<T, DIM> {
    fn dim(&self) -> Const<DIM> {
        Const::<DIM>
    }

    fn new(_dim: Const<DIM>) -> Self {
        ArrayVec::new()
    }

    fn len(&self) -> usize {
        self.len()
    }
}

#[derive(Clone, Debug)]
pub struct DynArray<T> {
    items: Box<[T]>,
}

impl<T> ArrayLike<Dyn, T> for DynArray<T> {
    fn dim(&self) -> Dyn {
        Dyn(self.items.len())
    }

    fn from_fn(dim: Dyn, f: impl FnMut() -> T) -> Self {
        let mut items = Box::new_uninit_slice(dim.0);

        // this will fully initialize the slice. if f panics along the way the drop
        // handler for it will drop all initialized values
        InitializeUninitSlice::new(&mut items).fill_with(f);

        let items = unsafe { items.assume_init() };
        Self { items }
    }

    #[inline]
    fn as_slice(&self) -> &[T] {
        &self.items[..]
    }

    #[inline]
    fn as_slice_mut(&mut self) -> &mut [T] {
        &mut self.items[..]
    }
}

impl<T> Deref for DynArray<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.items
    }
}

impl<T> DerefMut for DynArray<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.items
    }
}

#[derive(Clone, Debug)]
pub struct BoundedDynVec<T> {
    inner: Vec<T>,
}

impl<T> VecLike<Dyn, T> for BoundedDynVec<T> {
    fn dim(&self) -> Dyn {
        Dyn(self.inner.capacity())
    }

    fn new(dim: Dyn) -> Self {
        Self {
            inner: Vec::with_capacity(dim.0),
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

#[derive(Clone, Debug)]
pub struct BoundedDynDeque<T> {
    inner: VecDeque<T>,
}

impl<T> DequeLike<Dyn, T> for BoundedDynDeque<T> {
    fn dim(&self) -> Dyn {
        Dyn(self.inner.capacity())
    }

    fn new(dim: Dyn) -> Self {
        Self {
            inner: VecDeque::with_capacity(dim.0),
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn push_front(&mut self, value: T) -> Option<T> {
        assert!(self.inner.len() <= self.inner.capacity());

        let old_value = if self.inner.len() == self.inner.capacity() {
            self.inner.pop_back()
        }
        else {
            None
        };

        self.inner.push_front(value);

        old_value
    }

    fn push_back(&mut self, value: T) -> Option<T> {
        assert!(self.inner.len() <= self.inner.capacity());

        let old_value = if self.inner.len() == self.inner.capacity() {
            self.inner.pop_front()
        }
        else {
            None
        };

        self.inner.push_back(value);

        old_value
    }

    fn get(&mut self, index: usize) -> Option<&T> {
        self.inner.get(index)
    }
}

#[derive(Clone, Debug)]
pub struct BoundedConstDeque<const DIM: usize, T> {
    inner: ArrayDeque<T, DIM, arraydeque::Wrapping>,
}

impl<const DIM: usize, T> DequeLike<Const<DIM>, T> for BoundedConstDeque<DIM, T> {
    fn dim(&self) -> Const<DIM> {
        Const::<DIM>
    }

    fn new(_dim: Const<DIM>) -> Self {
        Self {
            inner: ArrayDeque::new(),
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    fn push_front(&mut self, value: T) -> Option<T> {
        self.inner.push_front(value)
    }

    fn push_back(&mut self, value: T) -> Option<T> {
        self.inner.push_back(value)
    }

    fn get(&mut self, index: usize) -> Option<&T> {
        self.inner.get(index)
    }
}

/// Helper to initialize slices `[MaybeUninit<T>]`.
///
/// It keeps track until which index the elements have been initialized. When
/// dropped it will drop all initialized items. This is so that in case of a
/// panic during initialization we make sure everything initialized will be
/// dropped.
///
/// Call [`Self::finish`] to finish the initialization (and thus not dropping
/// everything when this wrapper is dropped).
struct InitializeUninitSlice<'a, T> {
    items: &'a mut [MaybeUninit<T>],
    index: usize,
}

impl<'a, T> InitializeUninitSlice<'a, T> {
    pub fn new(items: &'a mut [MaybeUninit<T>]) -> Self {
        Self { items, index: 0 }
    }

    pub fn write(&mut self, value: T) {
        self.items[self.index].write(value);
        self.index += 1;
    }

    pub fn fill_with(mut self, mut f: impl FnMut() -> T) {
        while self.index < self.items.len() {
            self.write(f());
        }
        self.finish();
    }

    pub fn finish(self) {
        assert_eq!(self.index, self.items.len(), "slice not fully initialized");
        std::mem::forget(self)
    }
}

impl<'a, T> Drop for InitializeUninitSlice<'a, T> {
    fn drop(&mut self) {
        for item in &mut self.items[..self.index] {
            unsafe {
                item.assume_init_drop();
            }
        }
    }
}
