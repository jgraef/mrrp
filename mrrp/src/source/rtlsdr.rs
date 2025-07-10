use num_complex::Complex;
use rtlsdr_async as rtlsdr;

#[inline]
pub fn convert_iq(sample: rtlsdr::Iq) -> Complex<f32> {
    Complex {
        re: convert_scalar(sample.i),
        im: convert_scalar(sample.q),
    }
}

#[inline]
pub fn convert_scalar(x: u8) -> f32 {
    (x as f32 - 128.0) / 128.0
}
