mod read;
mod write;

use std::ops::Add;

pub use self::{
    read::*,
    write::*,
};

pub trait GetSampleRate {
    fn sample_rate(&self) -> f32;
}

impl<T: GetSampleRate> GetSampleRate for &T {
    #[inline]
    fn sample_rate(&self) -> f32 {
        (&**self).sample_rate()
    }
}

impl<T: GetSampleRate> GetSampleRate for &mut T {
    #[inline]
    fn sample_rate(&self) -> f32 {
        (&**self).sample_rate()
    }
}

pub trait StreamLength {
    fn remaining(&self) -> Remaining;

    #[inline]
    fn size_hint(&self) -> SizeHint {
        self.remaining().size_hint()
    }
}

impl<T> StreamLength for &T
where
    T: StreamLength + ?Sized,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        (&**self).remaining()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        (&**self).size_hint()
    }
}

impl<T> StreamLength for &mut T
where
    T: StreamLength + ?Sized,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        (&**self).remaining()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        (&**self).size_hint()
    }
}

pub trait FiniteStream: StreamLength {
    #[inline]
    fn len(&self) -> usize {
        match self.remaining() {
            Remaining::Finite { num_samples } => num_samples,
            Remaining::Infinite => panic!("stream marked as finite returned infinite length"),
            Remaining::Unknown => panic!("stream marked as finite returned unknown length"),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Remaining {
    Finite { num_samples: usize },
    Infinite,
    Unknown,
}

impl Remaining {
    #[inline]
    pub fn map(self, mut f: impl FnMut(usize) -> usize) -> Self {
        match self {
            Self::Finite { num_samples } => {
                Self::Finite {
                    num_samples: f(num_samples),
                }
            }
            Self::Infinite => Self::Infinite,
            Self::Unknown => Self::Unknown,
        }
    }

    #[inline]
    pub fn min(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Infinite, Self::Infinite) => Self::Infinite,
            (Self::Infinite, Self::Finite { num_samples })
            | (Self::Finite { num_samples }, Self::Infinite) => Self::Finite { num_samples },
            (Self::Finite { num_samples: left }, Self::Finite { num_samples: right }) => {
                Self::Finite {
                    num_samples: left.min(right),
                }
            }
        }
    }

    #[inline]
    pub fn size_hint(&self) -> SizeHint {
        match self {
            Self::Finite { num_samples } => {
                SizeHint {
                    lower_bound: *num_samples,
                    upper_bound: Some(*num_samples),
                }
            }
            Self::Infinite | Self::Unknown => {
                SizeHint {
                    lower_bound: 0,
                    upper_bound: None,
                }
            }
        }
    }

    #[inline]
    pub fn finite_length(&self) -> Option<usize> {
        match self {
            Remaining::Finite { num_samples } => Some(*num_samples),
            Remaining::Infinite | Remaining::Unknown => None,
        }
    }
}

impl Add<Self> for Remaining {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Remaining) -> Self::Output {
        match (self, rhs) {
            (Self::Infinite, _) | (_, Self::Infinite) => Self::Infinite,
            (Self::Unknown, _) | (_, Self::Unknown) => Self::Unknown,
            (Self::Finite { num_samples: left }, Self::Finite { num_samples: right }) => {
                Self::Finite {
                    num_samples: left + right,
                }
            }
        }
    }
}

impl Add<usize> for Remaining {
    type Output = Self;

    #[inline]
    fn add(self, rhs: usize) -> Self::Output {
        match self {
            Self::Finite { num_samples } => {
                Self::Finite {
                    num_samples: num_samples + rhs,
                }
            }
            Self::Infinite => Self::Infinite,
            Self::Unknown => Self::Unknown,
        }
    }
}

impl PartialEq<Self> for Remaining {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Infinite, Self::Infinite) => true,
            (Self::Finite { num_samples: left }, Self::Finite { num_samples: right }) => {
                left == right
            }
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SizeHint {
    pub lower_bound: usize,
    pub upper_bound: Option<usize>,
}

impl SizeHint {
    #[inline]
    pub fn buffer_size(&self, lower_bound_min: usize) -> usize {
        self.upper_bound
            .unwrap_or_else(|| self.lower_bound.max(lower_bound_min))
    }

    #[inline]
    pub fn min(&self, other: Self) -> Self {
        Self {
            lower_bound: self.lower_bound.min(other.lower_bound),
            upper_bound: self
                .upper_bound
                .zip(other.upper_bound)
                .map(|(left, right)| left.min(right)),
        }
    }
}

impl Add<Self> for SizeHint {
    type Output = Self;

    #[inline]
    fn add(self, rhs: Self) -> Self::Output {
        Self {
            lower_bound: self.lower_bound + rhs.lower_bound,
            upper_bound: self
                .upper_bound
                .zip(rhs.upper_bound)
                .map(|(left, right)| left + right),
        }
    }
}

impl Add<usize> for SizeHint {
    type Output = Self;

    fn add(self, rhs: usize) -> Self::Output {
        Self {
            lower_bound: self.lower_bound + rhs,
            upper_bound: self.upper_bound.map(|upper_bound| upper_bound + rhs),
        }
    }
}
