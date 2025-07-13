use std::{
    fmt::Display,
    ops::{
        Bound,
        Deref,
        RangeBounds,
    },
    sync::Arc,
};

use chrono::{
    DateTime,
    Local,
};
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrequencyBand {
    pub start: u32,
    pub end: u32,
}

impl FrequencyBand {
    #[inline(always)]
    pub fn center(&self) -> u32 {
        (self.start + self.end) / 2
    }

    #[inline(always)]
    pub fn bandwidth(&self) -> u32 {
        self.end - self.start
    }

    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let start = self.start.max(other.start);
        let end = self.end.min(other.end);
        (start < end).then(|| Self { start, end })
    }
}

impl RangeBounds<u32> for FrequencyBand {
    fn start_bound(&self) -> Bound<&u32> {
        Bound::Included(&self.start)
    }

    fn end_bound(&self) -> Bound<&u32> {
        Bound::Excluded(&self.end)
    }
}

#[inline(always)]
pub fn lerp(t: f32, a: f32, b: f32) -> f32 {
    (1.0 - t) * a + t * b
}

#[inline(always)]
pub fn unlerp(x: f32, a: f32, b: f32) -> f32 {
    (x - a) / (b - a)
}

fn min_max_float(
    iter: impl IntoIterator<Item = f32>,
    mut f: impl FnMut(f32, f32) -> bool,
) -> Option<f32> {
    let mut current_min = None;
    for x in iter {
        if current_min.map_or(true, |min| f(min, x)) {
            current_min = Some(x);
        }
    }
    current_min
}

#[inline(always)]
pub fn min_float(iter: impl IntoIterator<Item = f32>) -> Option<f32> {
    min_max_float(iter, |min, x| x < min)
}

#[inline(always)]
pub fn max_float(iter: impl IntoIterator<Item = f32>) -> Option<f32> {
    min_max_float(iter, |min, x| x > min)
}

#[derive(Clone, Debug)]
pub enum StaticOrArc<T: 'static> {
    Arc(Arc<T>),
    Static(&'static T),
}

impl<T: 'static> Deref for StaticOrArc<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        match self {
            StaticOrArc::Arc(value) => &**value,
            StaticOrArc::Static(value) => *value,
        }
    }
}

impl<T: 'static> From<Arc<T>> for StaticOrArc<T> {
    fn from(value: Arc<T>) -> Self {
        Self::Arc(value)
    }
}

impl<T: 'static> From<&'static T> for StaticOrArc<T> {
    fn from(value: &'static T) -> Self {
        Self::Static(value)
    }
}

pub fn format_frequency(frequency: u32) -> FormatFrequency {
    FormatFrequency {
        frequency,
        band: None,
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FormatFrequency {
    frequency: u32,
    band: Option<FrequencyBand>,
}

impl FormatFrequency {
    pub fn with_band(mut self, band: FrequencyBand) -> Self {
        self.band = Some(band);
        self
    }
}

impl Display for FormatFrequency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (divisor, prefix) = si_prefix(self.frequency);
        let divisor = divisor as f32;

        let precision = self
            .band
            .map(|band| (divisor / band.bandwidth() as f32).log10().ceil() as usize + 1)
            .or_else(|| f.precision())
            .unwrap_or(2);

        // todo: you could certainly do this without floats
        write!(
            f,
            "{:.precision$} {prefix}Hz",
            self.frequency as f32 / divisor
        )
    }
}

const SI_PREFIXES: &'static [(u32, &'static str)] =
    &[(1_000, "k"), (1_000_000, "M"), (1_000_000_000, "G")];

pub fn si_prefix(x: u32) -> (u32, &'static str) {
    SI_PREFIXES
        .iter()
        .rev()
        .copied()
        .find(|(n, _)| x > *n)
        .unwrap_or((1, ""))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Snapshot<T> {
    pub state: T,
    pub timestamp: DateTime<Local>,
}
