//! Conversions between color formats
//!
//! The conversion matrixes were adapted from [this website](https://web.archive.org/web/20180423091842/http://www.equasys.de/colorconversion.html). We made them affine and added the constant offsets into them.

use nalgebra::Matrix4;

#[derive(Clone, Copy, Debug)]
pub struct Conversion(Matrix4<f32>);

impl Conversion {
    #[inline]
    pub fn apply(&self, color: &[f32; 3]) -> [f32; 3] {
        let vector = self.0 * ::nalgebra::vector![color[0], color[1], color[2], 1.0];
        [vector[0], vector[1], vector[2]]
    }
}

macro_rules! conversion {
    ($($tt:tt)*) => {
        Conversion(::nalgebra::matrix![$($tt)*])
    };
}

pub const RGB_TO_YUV: Conversion = conversion![
    0.299, 0.587, 0.144, 0.0;
    -0.147, -0.289, 0.436, 0.0;
    0.615, -0.515, -0.1, 0.0;
    0.0, 0.0, 0.0, 1.0;
];

pub const YUV_TO_RGB: Conversion = conversion![
    1.0, 0.0, 1.14, 0.0;
    1.0, -0.395, -0.581, 0.0;
    1.0, 2.032, 0.0, 0.0;
    0.0, 0.0, 0.0, 1.0;
];

pub const RGB_TO_YCBCR_ITU: Conversion = conversion![
    0.257, 0.504, 0.098, 0.0625;
    -0.148, -0.291, 0.439, 0.5;
    0.439, -0.368, -0.071, 0.5;
    0.0, 0.0, 0.0, 1.0;
];

pub const YCBCR_TO_RGB_ITU: Conversion = conversion![
    1.164, 0.0, 1.596, -0.87975;
    1.164, -0.392, -0.813, 0.52975;
    1.164, 2.017, 0.0, -1.08125;
    0.0, 0.0, 0.0, 1.0;
];

pub const RGB_TO_YCBCR_FULL: Conversion = conversion![
    0.299, 0.587, 0.113, 0.0;
    -0.169, -0.331, 0.5, 0.5;
    0.5, -0.419, -0.081, 0.5;
    0.0, 0.0, 0.0, 1.0;
];

pub const YCBCR_TO_RGB_FULL: Conversion = conversion![
    1.0, 0.0, 1.4, -0.7;
    1.0, -0.343, -0.711, 0.527;
    1.0, 1.765, 0.0, -0.8825;
    0.0, 0.0, 0.0, 1.0;
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Luma(pub f32);

#[derive(Clone, Copy, Debug, Default)]
pub struct Rgb(pub [f32; 3]);

impl From<Luma> for Rgb {
    #[inline]
    fn from(value: Luma) -> Self {
        Self(std::array::repeat(value.0))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Yuv(pub [f32; 3]);

impl From<Luma> for Yuv {
    #[inline]
    fn from(value: Luma) -> Self {
        Self([value.0, 0.0, 0.0])
    }
}

impl From<Rgb> for Yuv {
    #[inline]
    fn from(value: Rgb) -> Self {
        Self(RGB_TO_YUV.apply(&value.0))
    }
}

impl From<Yuv> for Rgb {
    #[inline]
    fn from(value: Yuv) -> Self {
        Self(YUV_TO_RGB.apply(&value.0))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct YCbCr(pub [f32; 3]);

impl From<Luma> for YCbCr {
    #[inline]
    fn from(value: Luma) -> Self {
        Self([value.0, 0.0, 0.0])
    }
}

impl From<Rgb> for YCbCr {
    #[inline]
    fn from(value: Rgb) -> Self {
        Self(RGB_TO_YCBCR_ITU.apply(&value.0))
    }
}

impl From<YCbCr> for Rgb {
    #[inline]
    fn from(value: YCbCr) -> Self {
        Self(YCBCR_TO_RGB_ITU.apply(&value.0))
    }
}

#[cfg(feature = "image")]
impl From<Rgb> for image::Rgb<f32> {
    #[inline]
    fn from(value: Rgb) -> Self {
        Self(value.0)
    }
}

#[cfg(feature = "image")]
impl From<image::Rgb<f32>> for Rgb {
    #[inline]
    fn from(value: image::Rgb<f32>) -> Self {
        Rgb(value.0)
    }
}

#[cfg(feature = "image")]
impl From<Rgb> for image::Rgb<u8> {
    #[inline]
    fn from(value: Rgb) -> Self {
        Self(value.0.map(|x| (x * 255.0).clamp(0.0, 255.0) as u8))
    }
}

#[cfg(feature = "image")]
impl From<image::Rgb<u8>> for Rgb {
    #[inline]
    fn from(value: image::Rgb<u8>) -> Self {
        Self(value.0.map(|x| x as f32 / 255.0))
    }
}
