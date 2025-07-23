pub mod argmin;
pub mod equiripple_fft;
pub mod pm_remez;

pub trait FilterSpecification {
    fn frequency_response_at(&self, frequency: f32) -> Option<FrequencyResponseAt>;
    fn optimal_filter_length(&self) -> Option<usize>;

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

#[derive(Clone, Copy, Debug)]
pub struct FrequencyResponseAt {
    pub amplitude: f32,
    pub tolerance: f32,
}

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
}

impl FilterSpecification for Lowpass {
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

    fn optimal_filter_length(&self) -> Option<usize> {
        let n = (-20.0
            * (self.passband_tolerance * self.stopband_tolerance)
                .sqrt()
                .log10()
            - 13.0)
            / (14.6 * (self.stopband_start - self.passband_end));
        Some(n.ceil() as usize)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SampledIdealFrequencyResponse<S> {
    pub filter_specification: S,
    pub fft_size: usize,
}

impl<S> SampledIdealFrequencyResponse<S>
where
    S: FilterSpecification,
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
        FilterSpecification,
        Lowpass,
    };

    #[test]
    fn it_estimates_lowpass_filter_length_correctly() {
        let length = Lowpass {
            passband_end: 0.2,
            stopband_start: 0.3,
            passband_tolerance: 0.05,
            stopband_tolerance: 0.05,
        }
        .optimal_filter_length()
        .unwrap();

        assert_abs_diff_eq!(length, 9);
    }
}
