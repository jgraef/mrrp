use std::sync::Arc;

use argmin::{
    core::{
        CostFunction,
        Executor,
    },
    solver::particleswarm::ParticleSwarm,
};
use num_complex::Complex;
use num_traits::Zero;
use rustfft::{
    Fft,
    FftPlanner,
};

use crate::filter::design::{
    DesiredFrequencyResponse,
    ToConcreteFilterLength,
    fft_size_for_filter_length,
};

#[derive(Debug, thiserror::Error)]
#[error("optimizer error")]
pub enum Error {
    ArgminMath(#[from] argmin_math::Error),
    FilterLengthNotSpecified,
}

#[derive(Clone, Copy, Debug)]
pub struct FftBestFit<S> {
    pub filter_specification: S,
    pub filter_length: usize,
    pub fft_size: usize,
}

impl<S> FftBestFit<S>
where
    S: DesiredFrequencyResponse,
{
    pub fn particle_swarm_fft(&self, fft_planner: &mut FftPlanner<f32>) -> Result<Vec<f32>, Error> {
        let bounds = (
            vec![-100.0; self.filter_length],
            vec![100.0; self.filter_length],
        );

        let problem = FftBestFitProblem {
            parameters: &self,
            fft: fft_planner.plan_fft_forward(self.fft_size),
        };
        let solver = ParticleSwarm::new(bounds, 100);

        let executor = Executor::new(problem, solver).configure(|state| {
            // todo
            state.max_iters(100000).target_cost(10e-4)
        });

        let result = executor.run()?;
        println!("error: {}", result.state.best_cost);

        let filter_coefficients = result.state.best_individual.unwrap().position;

        Ok(filter_coefficients)
    }
}

#[derive(Clone, derive_more::Debug)]
struct FftBestFitProblem<'a, S> {
    parameters: &'a FftBestFit<S>,
    #[debug(skip)]
    fft: Arc<dyn Fft<f32>>,
}

impl<'a, S> CostFunction for FftBestFitProblem<'a, S>
where
    S: DesiredFrequencyResponse,
{
    type Param = Vec<f32>;
    type Output = f32;

    fn cost(&self, param: &Self::Param) -> Result<Self::Output, argmin_math::Error> {
        let mut h = param
            .iter()
            .copied()
            .map(Complex::from)
            .chain(std::iter::repeat(Zero::zero()))
            .take(self.parameters.fft_size)
            .collect::<Vec<_>>();

        self.fft.process(&mut h);

        let error = h
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let mut f = i as f32 / self.parameters.fft_size as f32;
                if f >= 0.5 {
                    f -= 1.0;
                }

                self.parameters
                    .filter_specification
                    .frequency_response_at(f)
                    .map(|h_id| (h_id.amplitude - h.norm()).powi(2))
                    .unwrap_or_default()
            })
            .sum::<f32>()
            / h.len() as f32;

        Ok(error)
    }
}

pub fn particle_swarm_fft<S>(
    filter_specification: S,
    filter_length: impl ToConcreteFilterLength<S>,
    fft_size: impl Into<Option<usize>>,
) -> Result<Vec<f32>, Error>
where
    S: DesiredFrequencyResponse,
{
    let filter_length = filter_length.to_concrete_filter_length(&filter_specification);

    let fft_size = fft_size
        .into()
        .unwrap_or_else(|| fft_size_for_filter_length(filter_length));

    let fft_best_fit = FftBestFit {
        filter_specification,
        filter_length,
        fft_size,
    };

    let mut fft_planner = FftPlanner::new();
    fft_best_fit.particle_swarm_fft(&mut fft_planner)
}

#[cfg(test)]
mod tests {

    use crate::filter::design::{
        Lowpass,
        Normalize,
    };

    #[test]
    fn particle_swarm_fft() {
        let filter_design = super::particle_swarm_fft(
            Lowpass::new(0.25, 0.1, 0.05, 0.05).assert_normalized(),
            11,
            None,
        )
        .unwrap();

        let h = filter_design;
        let n = (h.len() - 1) / 2;
        for (i, h) in h.iter().enumerate() {
            println!("{}: {h}", i as isize - n as isize);
        }
        todo!();
    }
}
