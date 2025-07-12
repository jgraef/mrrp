#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrequencyBand {
    pub start: u32,
    pub end: u32,
}

impl FrequencyBand {
    #[inline(always)]
    pub fn center(&self) -> u32 {
        (self.start + self.end) / 2
    }

    #[inline(always)]
    pub fn bandwidth(&self) -> u32 {
        self.end - self.start
    }
}

#[inline(always)]
pub fn lerp(t: f32, a: f32, b: f32) -> f32 {
    (1.0 - t) * a + t * b
}

fn min_max_float(
    iter: impl IntoIterator<Item = f32>,
    mut f: impl FnMut(f32, f32) -> bool,
) -> Option<f32> {
    let mut current_min = None;
    for x in iter {
        if current_min.map_or(true, |min| f(min, x)) {
            current_min = Some(x);
        }
    }
    current_min
}

#[inline(always)]
pub fn min_float(iter: impl IntoIterator<Item = f32>) -> Option<f32> {
    min_max_float(iter, |min, x| x < min)
}

#[inline(always)]
pub fn max_float(iter: impl IntoIterator<Item = f32>) -> Option<f32> {
    min_max_float(iter, |min, x| x > min)
}
