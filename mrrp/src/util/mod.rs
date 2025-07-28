use std::ops::{
    Bound,
    RangeBounds,
};

//pub mod array_vecdeque;
pub mod dim;

#[inline(always)]
pub fn lerp(t: f32, a: f32, b: f32) -> f32 {
    (1.0 - t) * a + t * b
}

#[inline(always)]
pub fn unlerp(x: f32, a: f32, b: f32) -> f32 {
    (x - a) / (b - a)
}

pub fn slice_bounds(range: impl RangeBounds<usize>, start: usize, end: usize) -> (usize, usize) {
    assert!(start <= end);

    let range_start = match range.start_bound().cloned() {
        Bound::Included(index) => start + index,
        Bound::Excluded(index) => start + index + 1,
        Bound::Unbounded => start,
    };

    let range_end = match range.end_bound().cloned() {
        Bound::Included(index) => start + index + 1,
        Bound::Excluded(index) => start + index,
        Bound::Unbounded => end,
    };

    (range_start, range_end)
}
