use std::ops::{
    Bound,
    RangeBounds,
};

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
