#[cfg(feature = "sat")]
pub mod sat;
pub mod signal;

#[inline]
pub fn lerp(t: f32, a: f32, b: f32) -> f32 {
    (1.0 - t) * a + t * b
}

#[inline]
pub fn unlerp(x: f32, a: f32, b: f32) -> f32 {
    (x - a) / (b - a)
}
