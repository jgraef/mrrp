//mod implementation;

use ::pm_remez::PMParameters;
use pm_remez::{
    Band,
    ParametersBuilder,
};

use crate::{
    filter::design::Lowpass,
    util::{
        lerp,
        unlerp,
    },
};

pub type Error = pm_remez::error::Error;

pub fn lowpass(filter_specification: Lowpass, filter_length: usize) -> Result<Vec<f32>, Error> {
    let bands = vec![
        Band::new(0.0, filter_specification.passband_end)?,
        Band::new(filter_specification.passband_end, 0.5)?,
    ];
    let mut parameters = PMParameters::new(
        filter_length,
        bands,
        |frequency| {
            // even though the ideal frequency response is not defined on the transition
            // band, the pm_remez crate will call this closure with values for it. we'll
            // just interpolate it between 1.0 and 0.0.

            // this first unlerps such that the transition band is mapped to [0.0, 1.0],
            // then it claps it to it and then lerps it to interpolate the transition band.
            // since the intermediate value is clamped to 0.0 if below passband_end or 1.0
            // if above stopband_start, this will produce the correct frequency response for
            // the filter.
            lerp(
                unlerp(
                    frequency,
                    filter_specification.passband_end,
                    filter_specification.stopband_start,
                )
                .clamp(0.0, 1.0),
                1.0,
                0.0,
            )
        },
        |frequency| {
            if frequency <= filter_specification.passband_end {
                filter_specification.stopband_tolerance / filter_specification.passband_tolerance
            }
            else if frequency >= filter_specification.stopband_start {
                1.0
            }
            else {
                //dbg!(filter_specification);
                //panic!("weights undefined for f={frequency}");
                1.0
            }
        },
    )?;
    parameters.set_symmetry(pm_remez::Symmetry::Even);

    Ok(pm_remez::pm_remez(&parameters)?.impulse_response)
}
