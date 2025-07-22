use core::fmt;
use std::{
    mem::MaybeUninit,
    ops::{
        Bound,
        Index,
        IndexMut,
    },
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
    pub fn slice_from_init(slice: &[S]) -> &UninitSlice<S> {
        unsafe { &*Self::pointer_from_init(slice as *const [S]) }
    }

    #[inline]
    pub fn slice_from_uninit(slice: &[MaybeUninit<S>]) -> &UninitSlice<S> {
        unsafe { &*Self::pointer_from_uninit(slice as *const [MaybeUninit<S>]) }
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
    pub fn copy_from_uninit(&mut self, source: &UninitSlice<S>) {
        assert_eq!(source.len(), self.len());
        for i in 0..source.len() {
            unsafe {
                self.0[i].write(source.0[i].assume_init_read());
            }
        }
    }

    #[inline]
    pub fn as_mut_ptr(&mut self) -> *mut S {
        self.0.as_mut_ptr() as *mut S
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub unsafe fn assume_init_ref(&self) -> &[S] {
        unsafe { self.0.assume_init_ref() }
    }

    #[inline]
    pub unsafe fn assume_init_mut(&mut self) -> &mut [S] {
        unsafe { self.0.assume_init_mut() }
    }

    pub fn fill_with(&mut self, mut fill: impl FnMut() -> S) {
        self.0.iter_mut().for_each(|sample| {
            sample.write(fill());
        });
    }

    pub unsafe fn assume_init_drop(&mut self) {
        self.0.iter_mut().for_each(|sample| unsafe {
            sample.assume_init_drop();
        });
    }
}

impl<S> fmt::Debug for UninitSlice<S> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("UninitSlice[...]").finish()
    }
}

macro_rules! impl_index_range {
    {$(
        $index:ty,
    )*} => {
        $(
            impl<S> Index<$index> for UninitSlice<S> {
                type Output = UninitSlice<S>;

                #[inline]
                fn index(&self, index: $index) -> &Self::Output {
                    UninitSlice::slice_from_uninit(&self.0[index])
                }
            }

            impl<S> IndexMut<$index> for UninitSlice<S> {
                #[inline]
                fn index_mut(&mut self, index: $index) -> &mut Self::Output {
                    UninitSlice::slice_mut_from_uninit(&mut self.0[index])
                }
            }
        )*
    };
}

impl_index_range! {
    (Bound<usize>, Bound<usize>),
    std::ops::Range<usize>,
    std::ops::RangeFrom<usize>,
    std::ops::RangeFull,
    std::ops::RangeInclusive<usize>,
    std::ops::RangeTo<usize>,
    std::ops::RangeToInclusive<usize>,
}

impl<S> Index<usize> for UninitSlice<S> {
    type Output = MaybeUninit<S>;

    #[inline]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}
