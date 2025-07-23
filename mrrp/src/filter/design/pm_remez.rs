use ::pm_remez::PMParameters;
use pm_remez::{
    Band,
    ParametersBuilder,
};

pub type Error = pm_remez::error::Error;

pub fn lowpass(
    cutoff_frequency: f32,
    transition_bandwidth: f32,
    filter_length: usize,
) -> Result<Vec<f32>, Error> {
    let bands = vec![
        Band::new(0.0, cutoff_frequency - 0.5 * transition_bandwidth)?,
        Band::new(cutoff_frequency + 0.5 * transition_bandwidth, 0.5)?,
    ];
    let mut parameters = PMParameters::new(
        filter_length,
        bands,
        |frequency| {
            if frequency <= cutoff_frequency {
                1.0
            }
            else {
                0.0
            }
        },
        |_| 1.0,
    )?;
    parameters.set_symmetry(pm_remez::Symmetry::Even);

    Ok(pm_remez::pm_remez(&parameters)?.impulse_response)
}
