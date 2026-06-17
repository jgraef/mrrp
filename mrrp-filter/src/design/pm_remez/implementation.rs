// original paper: https://web.ece.ucsb.edu/Faculty/Rabiner/ece259/Reprints/080_FIR_digital_filter_designs.pdf
// code + flowcharts: https://web.ece.ucsb.edu/Faculty/Rabiner/ece259/Reprints/062_computer%20program.pdf
// improvements: https://inria.hal.science/hal-01136005v5/document
// remez/slides: https://www.d-filter.ece.uvic.ca/SupMaterials-ne/Slides/df-ch15-S1-7P.pdf

use num_complex::Complex;
use num_traits::{
    Float,
    FloatConst,
};

use crate::filter::design::FilterSpecification;

#[derive(Debug, thiserror::Error)]
pub enum Error {}

#[derive(Clone, Debug)]
pub struct FilterDesign<T> {
    pub coefficients: Vec<T>,
    pub optimal_error: T,
    pub extremal_frequencies: Vec<T>,
}

#[derive(Clone, Copy, Debug)]
pub enum Symmetry {
    Positive,
    Negative,
}

enum FilterType {
    // case 1
    PositiveOdd,
    // case 2
    PositiveEven,
    // case 3
    NegativeOdd,
    // case 4
    NegativeEven,
}

impl FilterType {
    pub fn from_length_and_symmetry(filter_length: usize, symmetry: Symmetry) -> Self {
        match (symmetry, filter_length % 2 == 0) {
            (Symmetry::Positive, false) => Self::PositiveOdd,
            (Symmetry::Positive, true) => Self::PositiveEven,
            (Symmetry::Negative, false) => Self::NegativeOdd,
            (Symmetry::Negative, true) => Self::NegativeEven,
        }
    }

    pub fn symmetry(&self) -> Symmetry {
        match self {
            FilterType::PositiveOdd | FilterType::PositiveEven => Symmetry::Positive,
            FilterType::NegativeOdd | FilterType::NegativeEven => Symmetry::Negative,
        }
    }

    pub fn num_terms(&self, filter_length: usize) -> usize {
        match self {
            Self::PositiveOdd => {
                // sum of k=0..=n with n = (N - 1) / 2
                // thus num_terms = (N - 1) / 2 + 1 = (N + 1) / 2
                (filter_length + 1) / 2
            }
            _ => {
                // sum of k=1..=n with
                // n = (N - 1) / 2 for odd length
                // n = N / 2 otherwise
                // due to rounding we can just do
                filter_length / 2
            }
        }
    }

    fn linear_phase<T: Float + FloatConst>(&self, filter_length: usize, f: T) -> T {
        let l_half = match self.symmetry() {
            Symmetry::Positive => T::zero(),
            Symmetry::Negative => T::from(0.5).unwrap(),
        };
        let n = T::from(filter_length).unwrap();
        T::PI() * (l_half - (n - T::one()) * f)
    }

    pub fn recover_h<T: Float + FloatConst>(
        &self,
        filter_length: usize,
        abcd_tilda: &[T],
    ) -> Vec<T> {
        let mut h = vec![T::zero(); filter_length];
        let n = abcd_tilda.len();

        let one_half = T::from(0.5).unwrap();
        let one_quarter = T::from(0.25).unwrap();

        // maps from a~, b~ ,c~ , d~ to h[0..n-1]
        // this was a pain to work out from the paper
        match self {
            FilterType::PositiveOdd => {
                for k in 0..n-1 {
                    h[k] = one_half * abcd_tilda[n - k + 1];
                }
                h[n - 1] = abcd_tilda[0];
            }
            FilterType::PositiveEven => {
                h[0] = one_quarter * abcd_tilda[n - 1];
                for k in 1..n-1 {
                    h[k] = one_quarter *( abcd_tilda[n - k - 1] + abcd_tilda[n - k]);
                }
                h[n - 1] = one_half * abcd_tilda[0] + one_quarter * abcd_tilda[1];
            },
            FilterType::NegativeOdd => {
                h[0] = one_quarter * abcd_tilda[n - 1];
                h[1] = one_quarter * abcd_tilda[n - 2];
                for k in 2 .. n-1 {
                    h[k] = one_quarter * (abcd_tilda[n - k + 1] - abcd_tilda[n -k - 1]);
                }
                h[n-1] = one_half * abcd_tilda[0] - one_quarter * abcd_tilda[2];
            },
            FilterType::NegativeEven => {
                h[0] = one_quarter * abcd_tilda[n - 1];
                for k in 1..n-1 {
                    h[k] = one_quarter * (abcd_tilda[n - k + 1]  - abcd_tilda[n - k]);
                }
                h[n-1] = one_half * abcd_tilda[0] - one_quarter * abcd_tilda[1];
            },
        }

        // fills the second half of h using the given symmetry
        match self.symmetry() {
            Symmetry::Positive => {
                // h(k) = h(N - 1 - k)
                for k in 0..n {
                    h[filter_length - 1 - k] = h[k];
                }
            }
            Symmetry::Negative => {
                // h(k) = -h(N -1 - k)
                for k in 0..n {
                    h[filter_length - 1 - k] = -h[k];
                }
            }
        }

        h
    }

}

pub fn pm_remez<T, S>(
    filter_specification: S,
    filter_length: usize,
    symmetry: Symmetry,
) -> Result<FilterDesign<T>, Error>
where
    T: Float + FloatConst,
    S: FilterSpecification,
{
    let odd_length = filter_length % 2 == 1;

    todo!();
}
