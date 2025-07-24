//mod implementation;

use ::pm_remez::ParametersBuilder as _;

use crate::filter::design::FilterSpecification;

pub type Error = pm_remez::error::Error;

pub fn pm_remez<S>(filter_specification: S, filter_length: usize) -> Result<Vec<f32>, Error>
where
    S: FilterSpecification,
{
    let bands = filter_specification
        .defined_on()
        .map(|band| ::pm_remez::Band::new(band.start, band.end).unwrap())
        .collect();

    // even though the ideal frequency response is not defined on the transition
    // band, the pm_remez crate will call this closure with frequencies outside the
    // defined bands. we'll just return good defaults

    let mut parameters = ::pm_remez::PMParameters::new(
        filter_length,
        bands,
        |frequency| {
            filter_specification
                .frequency_response_at(frequency)
                .map(|response| response.amplitude)
                .unwrap_or(0.5)
        },
        |frequency| {
            filter_specification
                .frequency_response_at(frequency)
                .map(|response| 1.0 / response.tolerance)
                .unwrap_or(0.0)
        },
    )?;
    parameters.set_symmetry(pm_remez::Symmetry::Even);

    Ok(pm_remez::pm_remez(&parameters)?.impulse_response)
}
