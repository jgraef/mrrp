pub mod generator;
pub mod noise;
pub mod pcm;
pub mod sine;

#[inline(always)]
pub fn lerp(t: f32, a: f32, b: f32) -> f32 {
    (1.0 - t) * a + t * b
}

#[inline(always)]
pub fn unlerp(x: f32, a: f32, b: f32) -> f32 {
    (x - a) / (b - a)
}
