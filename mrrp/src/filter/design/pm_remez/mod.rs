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

    fn design_filter(&self, filter_specification: &F) -> Result<Self::Design, Self::Error> {
        let filter_length = self
            .filter_length
            .to_concrete_filter_length(filter_specification);
        pm_remez(filter_specification, filter_length)
    }
}

impl FilterDesign for RemezDesign {
    fn coefficients(&self) -> &[f32] {
        &self.impulse_response
    }
}

pub fn pm_remez<F>(filter_specification: &F, filter_length: usize) -> Result<RemezDesign, Error>
where
    F: DesiredFrequencyResponse + IsSymmetric,
{
    let bands = filter_specification
        .defined_on()
        .into_iter()
        .map(|band| ::pm_remez::Band::new(band.start, band.end).unwrap())
        .collect();
    dbg!(&bands);

    // even though the ideal frequency response is not defined on the transition
    // band, the pm_remez crate will call this closure with frequencies outside the
    // defined bands. we'll just return good defaults

    let mut parameters = ::pm_remez::PMParameters::new(
        filter_length,
        bands,
        |frequency| {
            filter_specification
                .frequency_response_at(frequency)
                .unwrap_or_else(|| panic!("desired magnitude response queried at: {frequency}"))
                .amplitude
        },
        |frequency| {
            1.0 / filter_specification
                .frequency_response_at(frequency)
                .unwrap_or_else(|| panic!("band weight queried at: {frequency}"))
                .tolerance
        },
    )?;
    parameters.set_symmetry(match filter_specification.symmetry() {
        Symmetry::Positive => pm_remez::Symmetry::Even,
        Symmetry::Negative => pm_remez::Symmetry::Odd,
    });

    pm_remez::pm_remez(&parameters)
}
