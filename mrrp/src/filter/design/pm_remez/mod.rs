//mod implementation;

use ::pm_remez::ParametersBuilder as _;

use crate::filter::design::{
    DesignAlgorithm,
    DesiredFrequencyResponse,
    Estimate,
    FilterDesign,
    IsSymmetric,
    Symmetry,
    ToConcreteFilterLength,
};

pub type Error = pm_remez::error::Error;
pub type RemezDesign = pm_remez::PMDesign<f32>;

#[derive(Clone, Copy, Debug)]
pub struct RemezAlgorithm<L> {
    pub filter_length: L,
    pub chebyshev_proxy_degree: usize,
    pub max_iterations: usize,
    pub flatness_threshold: f32,
}

impl<L> RemezAlgorithm<L> {
    pub fn with_filter_length(self, filter_length: usize) -> RemezAlgorithm<usize> {
        RemezAlgorithm {
            filter_length,
            chebyshev_proxy_degree: self.chebyshev_proxy_degree,
            max_iterations: self.max_iterations,
            flatness_threshold: self.flatness_threshold,
        }
    }
}

impl Default for RemezAlgorithm<Estimate> {
    fn default() -> Self {
        RemezAlgorithm {
            filter_length: Estimate,
            chebyshev_proxy_degree: 8,
            max_iterations: 100,
            flatness_threshold: 1e-3,
        }
    }
}

impl<F, L> DesignAlgorithm<F> for RemezAlgorithm<L>
where
    F: DesiredFrequencyResponse + IsSymmetric,
    L: ToConcreteFilterLength<F>,
{
    type Design = RemezDesign;
    type Error = Error;

    fn design_filter(&self, filter_specification: F) -> Result<Self::Design, Self::Error> {
        let filter_length = self
            .filter_length
            .to_concrete_filter_length(&filter_specification);
        pm_remez(filter_specification, filter_length)
    }
}

impl FilterDesign for RemezDesign {
    fn coefficients(&self) -> &[f32] {
        &self.impulse_response
    }
}

pub fn pm_remez<F, L>(filter_specification: F, filter_length: L) -> Result<RemezDesign, Error>
where
    F: DesiredFrequencyResponse + IsSymmetric,
    L: ToConcreteFilterLength<F>,
{
    let filter_length = filter_length.to_concrete_filter_length(&filter_specification);

    let bands = filter_specification
        .defined_on()
        .into_iter()
        .map(|band| ::pm_remez::Band::new(band.start, band.end))
        .collect::<Result<Vec<_>, _>>()?;

    // work around that get the frequency response at the band edge of the closest
    // band, if the given frequency doesn't fall inside band.
    //
    // this might get fixed ([issue][1])
    //
    // [1]: https://github.com/maia-sdr/pm-remez/issues/24#issuecomment-3117639764
    let closest_match = |frequency| {
        let (_, edge) = filter_specification
            .defined_on()
            .into_iter()
            .filter_map(|band| {
                if frequency < band.start {
                    Some((band.start - frequency, band.start))
                }
                else if frequency > band.end {
                    Some((frequency - band.end, band.end))
                }
                else {
                    None
                }
            })
            .min_by(|(d1, _): &(f32, f32), (d2, _): &(f32, f32)| {
                d1.partial_cmp(d2).expect("failed to compare band edges")
            })
            .expect("frequency response returned None for frequency inside bands");
        filter_specification
            .frequency_response_at(edge)
            .expect("frequency response undefined on band edge")
    };

    let mut parameters = ::pm_remez::PMParameters::new(
        filter_length,
        bands,
        |frequency| {
            filter_specification
                .frequency_response_at(frequency)
                .unwrap_or_else(|| closest_match(frequency))
                .amplitude
        },
        |frequency| {
            1.0 / filter_specification
                .frequency_response_at(frequency)
                .unwrap_or_else(|| closest_match(frequency))
                .tolerance
        },
    )?;
    parameters.set_symmetry(match filter_specification.symmetry() {
        Symmetry::Positive => pm_remez::Symmetry::Even,
        Symmetry::Negative => pm_remez::Symmetry::Odd,
    });

    pm_remez::pm_remez(&parameters)
}
