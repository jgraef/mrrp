use std::{
    fmt::Display,
    str::FromStr,
};

use anyhow::Error;
use num_traits::Float;

pub const PREFIXES: &[(i32, &'static str)] = &[
    (-15, "f"),
    (-12, "p"),
    (-9, "n"),
    (-6, "μ"),
    (-3, "m"),
    (0, ""),
    (3, "k"),
    (6, "M"),
    (9, "G"),
    (12, "T"),
    (15, "P"),
];

pub fn prefix<F>(value: F) -> Option<(i32, &'static str)>
where
    F: Float,
{
    let exponent = (value.abs().log10() / F::from(3.0f32).unwrap())
        .floor()
        .to_i32()
        .unwrap()
        * 3;
    PREFIXES.iter().find(|(e, _p)| exponent == *e).copied()
}

pub fn format<'a, F>(value: F, unit: &'a str) -> Formatter<'a, F>
where
    F: Display + Float,
{
    if let Some((exponent, si_prefix)) = prefix(value) {
        Formatter {
            value: value / F::from(10.0f32).unwrap().powi(exponent),
            si_prefix,
            unit,
        }
    }
    else {
        Formatter {
            value,
            si_prefix: "",
            unit,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Formatter<'a, F> {
    value: F,
    si_prefix: &'static str,
    unit: &'a str,
}

impl<'a, F> Display for Formatter<'a, F>
where
    F: Display,
{
    fn fmt(&self, mut f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(&mut f)?;
        write!(f, " {}{}", self.si_prefix, self.unit)?;
        Ok(())
    }
}

pub fn parse<F>(s: &str, unit: &str) -> Result<F, Error>
where
    F: Float + FromStr,
    Error: From<<F as FromStr>::Err>,
{
    let s = s.trim();
    let s = s.strip_suffix(unit).unwrap_or(s);

    for (exponent, si_prefix) in PREFIXES {
        if let Some(s) = s.strip_suffix(si_prefix)
            && !si_prefix.is_empty()
        {
            return parse_value(s, *exponent);
        }
    }

    // no SI prefix?
    parse_value(s, 0)
}

#[inline]
fn parse_value<F>(s: &str, exponent: i32) -> Result<F, Error>
where
    F: Float + FromStr,
    Error: From<<F as FromStr>::Err>,
{
    Ok(s.parse::<F>()? * F::from(10.0f32).unwrap().powi(exponent))
}

#[inline]
pub fn parse_frequency<F>(s: &str) -> Result<F, Error>
where
    F: Float + FromStr,
    Error: From<<F as FromStr>::Err>,
{
    parse(s, "Hz")
}

#[cfg(test)]
mod tests {
    use crate::util::si::prefix;

    #[test]
    fn it_determines_si_prefix() {
        assert_eq!(prefix(3.3e9), Some((9, "G")));
        assert_eq!(prefix(33.0e9), Some((9, "G")));
        assert_eq!(prefix(330.0e9), Some((9, "G")));
        assert_eq!(prefix(3.3e6), Some((6, "M")));
        assert_eq!(prefix(33.0e6), Some((6, "M")));
        assert_eq!(prefix(330.0e6), Some((6, "M")));
        assert_eq!(prefix(3.3e3), Some((3, "k")));
        assert_eq!(prefix(33.0e3), Some((3, "k")));
        assert_eq!(prefix(330.0e3), Some((3, "k")));

        assert_eq!(prefix(3.3), Some((0, "")));
        assert_eq!(prefix(33.0), Some((0, "")));
        assert_eq!(prefix(330.0), Some((0, "")));

        assert_eq!(prefix(3.3e-3), Some((-3, "m")));
        assert_eq!(prefix(33.0e-3), Some((-3, "m")));
        assert_eq!(prefix(330.0e-3), Some((-3, "m")));
        assert_eq!(prefix(3.3e-6), Some((-6, "μ")));
        assert_eq!(prefix(33.0e-6), Some((-6, "μ")));
        assert_eq!(prefix(330.0e-6), Some((-6, "μ")));
        assert_eq!(prefix(3.3e-9), Some((-9, "n")));
        assert_eq!(prefix(33.0e-9), Some((-9, "n")));
        assert_eq!(prefix(330.0e-9), Some((-9, "n")));
    }
}
