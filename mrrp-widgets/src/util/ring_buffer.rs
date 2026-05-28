#![allow(dead_code)]

use std::ops::{
    Bound,
    RangeBounds,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct RingBufferAllocator {
    state: State,
    capacity: u64,
}

impl RingBufferAllocator {
    pub fn new(capacity: u64) -> Self {
        Self {
            state: Default::default(),
            capacity,
        }
    }

    pub fn allocate(&mut self, mut size: u64) -> Option<Slice> {
        if size == 0 {
            return Some(Slice::default());
        }

        if self.capacity == 0 {
            return None;
        }

        match &mut self.state {
            State::Empty => {
                if size <= self.capacity {
                    if size == self.capacity {
                        self.state = State::Full { start_and_end: 0 };
                    }
                    else {
                        self.state = State::Single {
                            start: 0,
                            end: size,
                        };
                    }

                    Some(Slice::new(Range::new(0, size)))
                }
                else {
                    None
                }
            }
            State::Full { start_and_end: _ } => None,
            State::Single { start, end } => {
                if size <= *start + self.capacity - *end {
                    let n = size.min(self.capacity - *end);
                    let slice = Slice::new(Range::from_start_and_length(*end, n));

                    *end += n;
                    size -= n;

                    if n > 0 {
                        assert_eq!(*end, self.capacity);

                        if n == *start {
                            self.state = State::Full {
                                start_and_end: *start,
                            };
                        }
                        else {
                            self.state = State::Split {
                                start: *start,
                                end: n,
                            };
                        }

                        Some(slice.and(Range::new(0, size)))
                    }
                    else {
                        Some(slice)
                    }
                }
                else {
                    None
                }
            }
            State::Split { start, end } => {
                if size <= *start - *end {
                    let slice = Slice::new(Range::from_start_and_length(*end, size));
                    *end += size;

                    if *end == *start {
                        self.state = State::Full {
                            start_and_end: *start,
                        };
                    }

                    Some(slice)
                }
                else {
                    None
                }
            }
        }
    }

    pub fn free(&mut self, slice: Slice) -> bool {
        if slice.parts[0].is_empty() {
            assert!(slice.parts[1].is_empty());
            return true;
        }

        if self.capacity == 0 {
            return false;
        }

        match &mut self.state {
            State::Empty => false,
            State::Full {
                start_and_end: start_end_end,
            } => {
                if slice.parts[1].is_empty()
                    && slice.parts[0].start == *start_end_end
                    && slice.parts[0].end <= self.capacity
                {
                    assert!(slice.parts[0].end > *start_end_end);

                    if slice.parts[0].end == self.capacity {
                        if *start_end_end == 0 {
                            self.state = State::Empty;
                        }
                        else {
                            self.state = State::Single {
                                start: *start_end_end,
                                end: slice.parts[0].end,
                            };
                        }
                    }
                    else {
                        if *start_end_end == 0 {
                            self.state = State::Single {
                                start: slice.parts[0].end,
                                end: self.capacity,
                            };
                        }
                        else {
                            self.state = State::Split {
                                start: slice.parts[0].end,
                                end: *start_end_end,
                            }
                        }
                    }

                    true
                }
                else {
                    false
                }
            }
            State::Single { start, end } => {
                if slice.parts[1].is_empty()
                    && slice.parts[0].start == *start
                    && slice.parts[0].end <= *end
                {
                    if slice.parts[0].end == *end {
                        self.state = State::Empty;
                    }
                    else {
                        *start = slice.parts[0].end;
                    }

                    true
                }
                else {
                    false
                }
            }
            State::Split { start, end } => {
                if slice.parts[1].is_empty() {
                    if slice.parts[0].start == *start && slice.parts[0].end <= *end {
                        if slice.parts[0].end == *end {
                            self.state = State::Single {
                                start: 0,
                                end: *end,
                            };
                        }
                        else {
                            *start = slice.parts[0].end;
                        }

                        true
                    }
                    else {
                        false
                    }
                }
                else {
                    if slice.parts[0].start == *start
                        && slice.parts[0].end == self.capacity
                        && slice.parts[1].end <= *end
                    {
                        assert_eq!(slice.parts[1].start, 0);

                        if slice.parts[1].end == *end {
                            self.state = State::Empty;
                        }
                        else {
                            self.state = State::Single {
                                start: slice.parts[1].end,
                                end: *end,
                            };
                        }

                        true
                    }
                    else {
                        false
                    }
                }
            }
        }
    }

    pub fn allocated(&self) -> Slice {
        match self.state {
            State::Empty => Slice::default(),
            State::Full {
                start_and_end: start_end_end,
            } => {
                Slice::new(Range::new(0, start_end_end))
                    .and(Range::new(start_end_end, self.capacity))
            }
            State::Single { start, end } => Slice::new(Range::new(start, end)),
            State::Split { start, end } => {
                Slice::new(Range::new(start, self.capacity)).and(Range::new(0, end))
            }
        }
    }

    pub fn len(&self) -> u64 {
        match self.state {
            State::Empty => 0,
            State::Full { start_and_end: _ } => self.capacity,
            State::Single { start, end } => end - start,
            State::Split { start, end } => end + self.capacity - start,
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self.state, State::Empty)
    }

    pub fn available(&self) -> u64 {
        match self.state {
            State::Empty => self.capacity,
            State::Full { start_and_end: _ } => 0,
            State::Single { start, end } => start + self.capacity - end,
            State::Split { start, end } => start - end,
        }
    }

    pub fn is_full(&self) -> bool {
        matches!(self.state, State::Full { start_and_end: _ })
    }

    pub fn capacity(&self) -> u64 {
        self.capacity
    }
}

/// Tracks the allocated space
///
/// A naive approach would just keep track of start and end and determine if
/// it's split or contiguous by their ordering (e.g. split if end < start). But
/// for start == end it's ambigious if the ring buffer is empty or full.
/// Furthermore tracking all 4 cases individually makes implementation more
/// straight-forward.
///
/// There are four distinc cases:
///
/// 1. Completely empty
/// 2. Completely full
/// 3. Single contigious allocation and not completely full
/// 4. Two split allocations and not completely full
///
/// Under special circumstances some of these states overlap. They shall be
/// defined in order of priority - from high to low. And if 2 states would be
/// possible, the one with the higher priority is the only valid state.
///
/// E.g. A ring buffer with 0 capacity is both empty and full at the same time
/// (and can't be in the other states). We resolve this by declaring it to be
/// empty.
///
/// Or e.g. a `Single` could be defined such that the buffer is essentially
/// full. This must be avoided and the state set to `Full` instead.
#[derive(Clone, Copy, Debug, Default)]
enum State {
    /// +----------------------------------------------------------+
    /// |                                                          |
    /// +----------------------------------------------------------+
    ///
    /// When allocating from this state we start from the beginning of the
    /// buffer.
    #[default]
    Empty,
    /// +----------------------------------------------------------+
    /// |xxxxxxxxxxxxxxxxxxxxxxxxxxxx|xxxxxxxxxxxxxxxxxxxxxxxxxxxxx|
    /// +----------------------------------------------------------+
    ///                              ^
    ///                          end & start
    ///
    /// This state has a special case where `start_and_end` being 0 or
    /// `self.capacity` has the same meaning. It must be normalized to being 0
    /// in that case.
    Full { start_and_end: u64 },
    /// +----------------------------------------------------------+
    /// |                 |xxxxxxxxxxxxxxxxxx|                     |
    /// +----------------------------------------------------------+
    ///                   ^                  ^
    ///                 start               end
    ///
    /// If `start == end` this must become `Empty`.
    Single { start: u64, end: u64 },
    /// +----------------------------------------------------------+
    /// |xxxxxxxxxxxxxxxxx|                  |xxxxxxxxxxxxxxxxxxxxx|
    /// +----------------------------------------------------------+
    ///                   ^                   ^
    ///                  end                 start
    ///
    /// If `end == start` this must become `Full`.
    Split { start: u64, end: u64 },
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Slice {
    parts: [Range; 2],
}

impl Slice {
    fn new(range: Range) -> Self {
        Self {
            parts: [range, Default::default()],
        }
    }

    fn and(mut self, range: Range) -> Self {
        assert!(self.parts[1].is_empty());
        self.parts[1] = range;
        self
    }

    pub fn parts(&self) -> [Range; 2] {
        self.parts
    }

    pub fn iter(&self) -> impl Iterator<Item = Range> {
        self.parts.iter().filter(|range| !range.is_empty()).copied()
    }

    pub fn iter_with_source(&self) -> impl Iterator<Item = (std::ops::Range<usize>, Range)> {
        let mut offset = 0;
        self.iter().map(move |range| {
            let len = usize::try_from(range.len()).unwrap();
            let start = offset;
            let end = offset + len;
            offset += len;
            (start..end, range)
        })
    }
}

// this is basically std::ops::Range<u64>, but Copy!
#[derive(Clone, Copy, Debug, Default)]
pub struct Range {
    pub start: u64,
    pub end: u64,
}

impl Range {
    fn new(start: u64, end: u64) -> Self {
        Self { start, end }
    }

    fn from_start_and_length(start: u64, length: u64) -> Self {
        Self::new(start, start + length)
    }

    pub fn len(&self) -> u64 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl RangeBounds<u64> for Range {
    fn start_bound(&self) -> Bound<&u64> {
        Bound::Included(&self.start)
    }

    fn end_bound(&self) -> Bound<&u64> {
        Bound::Excluded(&self.end)
    }
}
