use core::fmt;
use std::{
    mem::MaybeUninit,
    sync::Arc,
};

#[repr(transparent)]
pub struct UninitSlice<S>([MaybeUninit<S>]);

impl<S> UninitSlice<S> {
    #[inline]
    pub fn pointer_from_init(pointer: *const [S]) -> *const UninitSlice<S> {
        Self::pointer_from_uninit(pointer as _)
    }

    #[inline]
    pub fn pointer_mut_from_init(pointer: *mut [S]) -> *mut UninitSlice<S> {
        Self::pointer_mut_from_uninit(pointer as _)
    }

    #[inline]
    pub fn pointer_from_uninit(pointer: *const [MaybeUninit<S>]) -> *const UninitSlice<S> {
        pointer as _
    }

    #[inline]
    pub fn pointer_mut_from_uninit(pointer: *mut [MaybeUninit<S>]) -> *mut UninitSlice<S> {
        pointer as _
    }

    #[inline]
    pub fn slice_mut_from_init(slice: &mut [S]) -> &mut UninitSlice<S> {
        unsafe { &mut *Self::pointer_mut_from_init(slice as *mut [S]) }
    }

    #[inline]
    pub fn slice_mut_from_uninit(slice: &mut [MaybeUninit<S>]) -> &mut UninitSlice<S> {
        unsafe { &mut *Self::pointer_mut_from_uninit(slice as *mut [MaybeUninit<S>]) }
    }

    #[inline]
    pub fn box_from_uninit(value: Box<[MaybeUninit<S>]>) -> Box<UninitSlice<S>> {
        let (pointer, alloc) = Box::into_raw_with_allocator(value);
        unsafe { Box::from_raw_in(UninitSlice::pointer_mut_from_uninit(pointer), alloc) }
    }

    #[inline]
    pub fn box_from_init(value: Box<[S]>) -> Box<UninitSlice<S>> {
        let (pointer, alloc) = Box::into_raw_with_allocator(value);
        unsafe { Box::from_raw_in(UninitSlice::pointer_mut_from_init(pointer), alloc) }
    }

    #[inline]
    pub fn box_new(length: usize) -> Box<UninitSlice<S>> {
        Self::box_from_uninit(Box::new_uninit_slice(length))
    }

    #[inline]
    pub fn arc_from_init(value: Arc<[S]>) -> Arc<UninitSlice<S>> {
        let (pointer, alloc) = Arc::into_raw_with_allocator(value);
        unsafe { Arc::from_raw_in(UninitSlice::pointer_from_init(pointer), alloc) }
    }

    #[inline]
    pub fn arc_from_uninit(value: Arc<[MaybeUninit<S>]>) -> Arc<UninitSlice<S>> {
        let (pointer, alloc) = Arc::into_raw_with_allocator(value);
        unsafe { Arc::from_raw_in(UninitSlice::pointer_from_uninit(pointer), alloc) }
    }

    #[inline]
    pub fn arc_new(length: usize) -> Arc<UninitSlice<S>> {
        Self::arc_from_uninit(Arc::new_uninit_slice(length))
    }

    #[inline]
    pub fn write_sample(&mut self, index: usize, sample: S) {
        self.0[index] = MaybeUninit::new(sample);
    }

    #[inline]
    pub fn clone_from_slice(&mut self, source: &[S])
    where
        S: Clone,
    {
        self.0.write_clone_of_slice(source);
    }

    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut S {
        self.0.as_mut_ptr() as *mut S
    }

    #[inline]
    pub fn as_uninit_slice_mut(&mut self) -> &mut [MaybeUninit<S>] {
        &mut self.0
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub unsafe fn assume_init_subslice(&self, start: usize, length: usize) -> &[S] {
        unsafe { self.0[start..][..length].assume_init_ref() }
    }
}

impl<S> fmt::Debug for UninitSlice<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UninitSlice[...]").finish()
    }
}
