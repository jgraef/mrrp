use crate::{
    GetSampleRate,
    filter::fir::FirFilter,
};

pub mod argmin;
pub mod equiripple_fft;
pub mod pm_remez;

pub trait DesiredFrequencyResponse {
    fn defined_on(&self) -> impl IntoIterator<Item = Band>;
    fn frequency_response_at(&self, frequency: f32) -> Option<FrequencyResponseAt>;

    fn sampled(self, fft_size: usize) -> SampledIdealFrequencyResponse<Self>
    where
        Self: Sized,
    {
        SampledIdealFrequencyResponse {
            filter_specification: self,
            fft_size,
        }
    }
}

/// Trait for specifying the required symmetry for a filter design
pub trait IsSymmetric {
    fn symmetry(&self) -> Symmetry;
}

/// Marker trait indicating that a filter speficiation is normalized to cycles
/// per sample.
pub trait IsNormalized {}

/// Trait to normalize a filter specification relative to a reference frequency.
pub trait Normalize: Sized {
    type Normalized: IsNormalized;

    fn normalize(self, reference: f32) -> Self::Normalized;

    #[inline]
    fn assert_normalized(self) -> Self::Normalized {
        self.normalize(1.0)
    }

    #[inline]
    fn normalize_with<R: GetSampleRate>(self, reference: &R) -> Self::Normalized {
        self.normalize(reference.sample_rate())
    }
}

/// Trait for filter specifications to define their optimal filter length.
///
/// Since this is usually a heuristic depending on normalized frequencies, this
/// is usually only defined on a normalized filter specification.
pub trait EstimateFilterLength {
    fn estimate_filter_length(&self) -> usize;
}

/// Trait for algorithms that turn filter specifications into filter designs
/// (i.e. their coefficients).
pub trait DesignAlgorithm<F>
where
    F: ?Sized,
{
    type Design: FilterDesign;
    type Error: std::error::Error;

    fn design_filter(&self, filter_specification: F) -> Result<Self::Design, Self::Error>;
}

pub trait FilterDesign {
    fn coefficients(&self) -> &[f32];

    #[inline]
    fn filter_length(&self) -> usize {
        self.coefficients().len()
    }

    #[inline]
    fn fir_filter<S>(&self) -> FirFilter<S, f32> {
        FirFilter::new(self.coefficients().to_owned())
    }
}

impl FilterDesign for Vec<f32> {
    #[inline]
    fn coefficients(&self) -> &[f32] {
        &self
    }

    #[inline]
    fn filter_length(&self) -> usize {
        self.len()
    }
}

/// Values that can be turned into a concrete filter length given a filter
/// specification
///
/// This is implemented for `usize` and [`Estimate`] if the filter specification
/// implements [`EstimateFilterLength`].
pub trait ToConcreteFilterLength<F> {
    fn to_concrete_filter_length(&self, filter_specification: &F) -> usize;
}

impl<F> ToConcreteFilterLength<F> for usize {
    #[inline]
    fn to_concrete_filter_length(&self, _filter_specification: &F) -> usize {
        *self
    }
}

/// Placeholder for values that should be estimated
///
/// # TODO
///
/// This should be moved to utils
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Estimate;

impl<F> ToConcreteFilterLength<F> for Estimate
where
    F: EstimateFilterLength,
{
    #[inline]
    fn to_concrete_filter_length(&self, filter_specification: &F) -> usize {
        filter_specification.estimate_filter_length()
    }
}

/// Symmetry of a filter
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Symmetry {
    /// Type I and II
    Positive,
    /// Type III and IV
    Negative,
}

/// Frequency interval of a band.
///
/// Some algorithms need to know the frequency bands for which the desired
/// frequency response is defined.
#[derive(Clone, Copy, Debug)]
pub struct Band {
    pub start: f32,
    pub end: f32,
}

/// The desired frequency response at a specific frequency.
#[derive(Clone, Copy, Debug)]
pub struct FrequencyResponseAt {
    pub amplitude: f32,
    pub tolerance: f32,
}

/// A generic wrapper for filter specifications that are garantueed to be
/// normalized
#[derive(Clone, Copy, Debug)]
pub struct Normalized<F>(pub F);

impl<F> IsNormalized for Normalized<F> {}

impl<F> Normalize for Normalized<F> {
    type Normalized = Self;

    #[inline]
    fn normalize(self, _sample_rate: f32) -> Self::Normalized {
        self
    }
}

impl<F> DesiredFrequencyResponse for Normalized<F>
where
    F: DesiredFrequencyResponse,
{
    #[inline]
    fn defined_on(&self) -> impl IntoIterator<Item = Band> {
        self.0.defined_on()
    }

    #[inline]
    fn frequency_response_at(&self, frequency: f32) -> Option<FrequencyResponseAt> {
        self.0.frequency_response_at(frequency)
    }
}

impl<F> EstimateFilterLength for Normalized<F>
where
    F: EstimateFilterLength,
{
    #[inline]
    fn estimate_filter_length(&self) -> usize {
        self.0.estimate_filter_length()
    }
}

impl<F> IsSymmetric for Normalized<F>
where
    F: IsSymmetric,
{
    #[inline]
    fn symmetry(&self) -> Symmetry {
        self.0.symmetry()
    }
}

/// A low-pass filter.
#[derive(Clone, Copy, Debug)]
pub struct Lowpass {
    pub passband_end: f32,
    pub stopband_start: f32,
    pub passband_tolerance: f32,
    pub stopband_tolerance: f32,
}

impl Lowpass {
    pub fn new(
        cutoff_frequency: f32,
        transition_bandwidth: f32,
        passband_tolerance: f32,
        stopband_tolerance: f32,
    ) -> Self {
        let half_transition_bandwidth = 0.5 * transition_bandwidth;
        Self {
            passband_end: cutoff_frequency - half_transition_bandwidth,
            stopband_start: cutoff_frequency + half_transition_bandwidth,
            passband_tolerance,
            stopband_tolerance,
        }
    }

    pub fn cutoff_frequency(&self) -> f32 {
        0.5 * (self.passband_end + self.stopband_start)
    }

    pub fn transition_bandwidth(&self) -> f32 {
        self.stopband_start - self.passband_end
    }
}

impl Normalize for Lowpass {
    type Normalized = Normalized<Self>;

    fn normalize(self, sample_rate: f32) -> Self::Normalized {
        Normalized(Self {
            passband_end: self.passband_end / sample_rate,
            stopband_start: self.stopband_start / sample_rate,
            passband_tolerance: self.passband_tolerance,
            stopband_tolerance: self.stopband_tolerance,
        })
    }
}

impl DesiredFrequencyResponse for Lowpass {
    fn defined_on(&self) -> impl IntoIterator<Item = Band> {
        [
            Band {
                start: 0.0,
                end: self.passband_end,
            },
            Band {
                start: self.stopband_start,
                end: 0.5,
            },
        ]
    }

    fn frequency_response_at(&self, frequency: f32) -> Option<FrequencyResponseAt> {
        if frequency <= self.passband_end {
            Some(FrequencyResponseAt {
                amplitude: 1.0,
                tolerance: self.passband_tolerance,
            })
        }
        else if frequency >= self.stopband_start {
            Some(FrequencyResponseAt {
                amplitude: 0.0,
                tolerance: self.stopband_tolerance,
            })
        }
        else {
            None
        }
    }
}

impl EstimateFilterLength for Normalized<Lowpass> {
    fn estimate_filter_length(&self) -> usize {
        let n = (-20.0
            * (self.0.passband_tolerance * self.0.stopband_tolerance)
                .sqrt()
                .log10()
            - 13.0)
            / (14.6 * (self.0.stopband_start - self.0.passband_end));
        n.ceil() as usize
    }
}

impl IsSymmetric for Lowpass {
    fn symmetry(&self) -> Symmetry {
        Symmetry::Positive
    }
}

/*
impl<R> MakeFilter<R> for Lowpass where R: GetSampleRate {
    type Filter = FirFiltered<R>;

    fn make_filter(&self, input: &R) -> Self::Filter {
        //self.normalize_with(input).make_filter(input)
        todo!();
    }
}
     */

#[derive(Clone, Copy, Debug)]
pub struct SampledIdealFrequencyResponse<S> {
    pub filter_specification: S,
    pub fft_size: usize,
}

impl<S> SampledIdealFrequencyResponse<S>
where
    S: DesiredFrequencyResponse,
{
    fn get(&self, index: usize) -> Option<FrequencyResponseAt> {
        let frequency = self.frequency(index).abs();
        self.filter_specification.frequency_response_at(frequency)
    }

    fn frequency(&self, index: usize) -> f32 {
        let mut frequency = index as f32 / self.fft_size as f32;
        if frequency >= 0.5 {
            frequency -= 1.0;
        }
        frequency
    }

    fn iter(&self) -> impl Iterator<Item = Option<FrequencyResponseAt>> {
        (0..self.fft_size).map(|i| self.get(i))
    }
}

pub fn fft_size_for_filter_length(length: usize) -> usize {
    (5 * (length - 1) + 1).next_power_of_two()
}

#[cfg(test)]
mod tests {
    use approx::assert_abs_diff_eq;

    use crate::filter::design::{
        EstimateFilterLength,
        Lowpass,
        Normalize,
    };

    #[test]
    fn it_estimates_lowpass_filter_length_correctly() {
        let length = Lowpass {
            passband_end: 0.2,
            stopband_start: 0.3,
            passband_tolerance: 0.05,
            stopband_tolerance: 0.05,
        }
        .assert_normalized()
        .estimate_filter_length();

        assert_abs_diff_eq!(length, 9);
    }
}
