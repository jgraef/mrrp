//! Mode specifications
//!
//! Adapted from [here][1]. [Vis codes][2]
//!
//! [1]: https://github.com/windytan/slowrx/blob/master/modespec.c
//! [2]: https://web.archive.org/web/20050306193820/http://www.tima.com/~djones/vis.txt

use std::{
    collections::HashMap,
    sync::OnceLock,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorFormat {
    Gbr,
    Rgb,
    Yuv,
    Gray,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct VisCode(u8);

impl VisCode {
    #[inline]
    pub const fn new(value: u8) -> Option<Self> {
        if value & 0x80 == 0 {
            Some(Self(value))
        }
        else {
            None
        }
    }

    #[inline]
    pub const fn new_unchecked(value: u8) -> Self {
        Self(value)
    }

    #[inline]
    pub fn get(&self) -> u8 {
        self.0
    }

    #[inline]
    pub fn get_bit(&self, bit: u8) -> bool {
        assert!(bit < 7);
        (self.0 >> bit) & 1 != 0
    }

    #[inline]
    pub fn parity(&self) -> bool {
        let parity = (self.0 >> 6)
            ^ (self.0 >> 5)
            ^ (self.0 >> 4)
            ^ (self.0 >> 3)
            ^ (self.0 >> 2)
            ^ (self.0 >> 1)
            ^ self.0;
        parity & 1 != 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ModeSpecification {
    pub name: &'static str,
    pub short_name: &'static str,
    pub sync_time: f32,
    pub porch_time: f32,
    pub sep_time: f32,
    pub pixel_time: f32,
    pub line_time: f32,
    pub pixels_per_line: usize,
    pub num_lines: usize,
    pub line_height: u32,
    pub color_format: ColorFormat,
    pub vis_code: VisCode,
}

impl ModeSpecification {
    // N7CXI, 2000
    pub const M1: Self = Self {
        name: "Martin M1",
        short_name: "M1",
        sync_time: 4.862e-3,
        porch_time: 0.572e-3,
        sep_time: 0.572e-3,
        pixel_time: 0.4576e-3,
        line_time: 446.446e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Gbr,
        vis_code: VisCode(0x2c),
    };

    /// N7CXI, 2000
    pub const M2: Self = Self {
        name: "Martin M2",
        short_name: "M2",
        sync_time: 4.862e-3,
        porch_time: 0.572e-3,
        sep_time: 0.572e-3,
        pixel_time: 0.2288e-3,
        line_time: 226.7986e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Gbr,
        vis_code: VisCode(0x28),
    };

    /// KB4YZ, 1999
    pub const M3: Self = Self {
        name: "Martin M3",
        short_name: "M3",
        sync_time: 4.862e-3,
        porch_time: 0.572e-3,
        sep_time: 0.572e-3,
        pixel_time: 0.2288e-3,
        line_time: 446.446e-3,
        pixels_per_line: 320,
        num_lines: 128,
        line_height: 2,
        color_format: ColorFormat::Gbr,
        vis_code: VisCode(0x24),
    };

    /// KB4YZ, 1999
    pub const M4: Self = Self {
        name: "Martin M4",
        short_name: "M4",
        sync_time: 4.862e-3,
        porch_time: 0.572e-3,
        sep_time: 0.572e-3,
        pixel_time: 0.2288e-3,
        line_time: 226.7986e-3,
        pixels_per_line: 320,
        num_lines: 128,
        line_height: 2,
        color_format: ColorFormat::Gbr,
        vis_code: VisCode(0x20),
    };

    /// N7CXI, 2000
    pub const S1: Self = Self {
        name: "Scottie S1",
        short_name: "S1",
        sync_time: 9e-3,
        porch_time: 1.5e-3,
        sep_time: 1.5e-3,
        pixel_time: 0.4320e-3,
        line_time: 428.38e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Gbr,
        vis_code: VisCode(0x3c),
    };

    /// N7CXI, 2000
    pub const S2: Self = Self {
        name: "Scottie S2",
        short_name: "S2",
        sync_time: 9e-3,
        porch_time: 1.5e-3,
        sep_time: 1.5e-3,
        pixel_time: 0.2752e-3,
        line_time: 277.692e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Gbr,
        vis_code: VisCode(0x38),
    };

    /// N7CXI, 2000
    pub const SDX: Self = Self {
        name: "Scottie DX",
        short_name: "SDX",
        sync_time: 9e-3,
        porch_time: 1.5e-3,
        sep_time: 1.5e-3,
        pixel_time: 1.08053e-3,
        line_time: 1050.3e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Gbr,
        vis_code: VisCode(0x4c),
    };

    /// N7CXI, 2000
    pub const R72: Self = Self {
        name: "Robot 72",
        short_name: "R72",
        sync_time: 9e-3,
        porch_time: 3e-3,
        sep_time: 4.7e-3,
        pixel_time: 0.2875e-3,
        line_time: 300e-3,
        pixels_per_line: 320,
        num_lines: 240,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x0c),
    };

    /// N7CXI, 2000
    pub const R36: Self = Self {
        name: "Robot 36",
        short_name: "R36",
        sync_time: 9e-3,
        porch_time: 3e-3,
        sep_time: 6e-3,
        pixel_time: 0.1375e-3,
        line_time: 150e-3,
        pixels_per_line: 320,
        num_lines: 240,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x08),
    };

    /// N7CXI, 2000
    pub const R24: Self = Self {
        name: "Robot 24",
        short_name: "R24",
        sync_time: 9e-3,
        porch_time: 3e-3,
        sep_time: 6e-3,
        pixel_time: 0.1375e-3,
        line_time: 150e-3,
        pixels_per_line: 320,
        num_lines: 240,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x04),
    };

    /// N7CXI, 2000
    pub const R24BW: Self = Self {
        name: "Robot 24 B/W",
        short_name: "R24Gray",
        sync_time: 7e-3,
        porch_time: 0e-3,
        sep_time: 0e-3,
        pixel_time: 0.291e-3,
        line_time: 100e-3,
        pixels_per_line: 320,
        num_lines: 240,
        line_height: 1,
        color_format: ColorFormat::Gray,
        vis_code: VisCode(0x0a),
    };

    /// N7CXI, 2000
    pub const R12BW: Self = Self {
        name: "Robot 12 B/W",
        short_name: "R12Gray",
        sync_time: 7e-3,
        porch_time: 0e-3,
        sep_time: 0e-3,
        pixel_time: 0.291e-3,
        line_time: 100e-3,
        pixels_per_line: 320,
        num_lines: 120,
        line_height: 2,
        color_format: ColorFormat::Gray,
        vis_code: VisCode(0x06),
    };

    /// N7CXI, 2000
    pub const R8BW: Self = Self {
        name: "Robot 8 B/W",
        short_name: "R8Gray",
        sync_time: 7e-3,
        porch_time: 0e-3,
        sep_time: 0e-3,
        pixel_time: 0.1871875e-3,
        line_time: 66.9e-3,
        pixels_per_line: 320,
        num_lines: 120,
        line_height: 2,
        color_format: ColorFormat::Gray,
        vis_code: VisCode(0x02),
    };

    /// KB4YZ, 1999
    pub const W2120: Self = Self {
        name: "Wraase SC-2 120",
        short_name: "W2120",
        sync_time: 5.5225e-3,
        porch_time: 0.5e-3,
        sep_time: 0e-3,
        pixel_time: 0.489039081e-3,
        line_time: 475.530018e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Rgb,
        vis_code: VisCode(0x3f),
    };

    /// N7CXI, 2000
    pub const W2180: Self = Self {
        name: "Wraase SC-2 180",
        short_name: "W2180",
        sync_time: 5.5225e-3,
        porch_time: 0.5e-3,
        sep_time: 0e-3,
        pixel_time: 0.734532e-3,
        line_time: 711.0225e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Rgb,
        vis_code: VisCode(0x37),
    };

    /// N7CXI, 2000
    pub const PD50: Self = Self {
        name: "PD-50",
        short_name: "PD50",
        sync_time: 20e-3,
        porch_time: 2.08e-3,
        sep_time: 0e-3,
        pixel_time: 0.286e-3,
        line_time: 388.16e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x5d),
    };

    /// N7CXI, 2000
    pub const PD90: Self = Self {
        name: "PD-90",
        short_name: "PD90",
        sync_time: 20e-3,
        porch_time: 2.08e-3,
        sep_time: 0e-3,
        pixel_time: 0.532e-3,
        line_time: 703.04e-3,
        pixels_per_line: 320,
        num_lines: 256,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x63),
    };

    /// N7CXI, 2000
    pub const PD120: Self = Self {
        name: "PD-120",
        short_name: "PD120",
        sync_time: 20e-3,
        porch_time: 2.08e-3,
        sep_time: 0e-3,
        pixel_time: 0.19e-3,
        line_time: 508.48e-3,
        pixels_per_line: 640,
        num_lines: 496,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x5f),
    };

    /// N7CXI, 2000
    pub const PD160: Self = Self {
        name: "PD-160",
        short_name: "PD160",
        sync_time: 20e-3,
        porch_time: 2.08e-3,
        sep_time: 0e-3,
        pixel_time: 0.382e-3,
        line_time: 804.416e-3,
        pixels_per_line: 512,
        num_lines: 400,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x62),
    };

    /// N7CXI, 2000
    pub const PD180: Self = Self {
        name: "PD-180",
        short_name: "PD180",
        sync_time: 20e-3,
        porch_time: 2.08e-3,
        sep_time: 0e-3,
        pixel_time: 0.286e-3,
        line_time: 754.24e-3,
        pixels_per_line: 640,
        num_lines: 496,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x60),
    };

    /// N7CXI, 2000
    pub const PD240: Self = Self {
        name: "PD-240",
        short_name: "PD240",
        sync_time: 20e-3,
        porch_time: 2.08e-3,
        sep_time: 0e-3,
        pixel_time: 0.382e-3,
        line_time: 1000e-3,
        pixels_per_line: 640,
        num_lines: 496,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x61),
    };

    /// N7CXI, 2000
    pub const PD290: Self = Self {
        name: "PD-290",
        short_name: "PD290",
        sync_time: 20e-3,
        porch_time: 2.08e-3,
        sep_time: 0e-3,
        pixel_time: 0.286e-3,
        line_time: 937.28e-3,
        pixels_per_line: 800,
        num_lines: 616,
        line_height: 1,
        color_format: ColorFormat::Yuv,
        vis_code: VisCode(0x5e),
    };

    /// N7CXI, 2000
    pub const P3: Self = Self {
        name: "Pasokon P3",
        short_name: "P3",
        sync_time: 5.208e-3,
        porch_time: 1.042e-3,
        sep_time: 1.042e-3,
        pixel_time: 0.2083e-3,
        line_time: 409.375e-3,
        pixels_per_line: 640,
        num_lines: 496,
        line_height: 1,
        color_format: ColorFormat::Rgb,
        vis_code: VisCode(0x71),
    };

    /// N7CXI, 2000
    pub const P5: Self = Self {
        name: "Pasokon P5",
        short_name: "P5",
        sync_time: 7.813e-3,
        porch_time: 1.563e-3,
        sep_time: 1.563e-3,
        pixel_time: 0.3125e-3,
        line_time: 614.065e-3,
        pixels_per_line: 640,
        num_lines: 496,
        line_height: 1,
        color_format: ColorFormat::Rgb,
        vis_code: VisCode(0x72),
    };

    /// N7CXI, 2000
    pub const P7: Self = Self {
        name: "Pasokon P7",
        short_name: "P7",
        sync_time: 10.417e-3,
        porch_time: 2.083e-3,
        sep_time: 2.083e-3,
        pixel_time: 0.4167e-3,
        line_time: 818.747e-3,
        pixels_per_line: 640,
        num_lines: 496,
        line_height: 1,
        color_format: ColorFormat::Rgb,
        vis_code: VisCode(0x73),
    };
}

pub fn builtin_mode_specification(vis_code: VisCode) -> Option<&'static ModeSpecification> {
    static MAP: OnceLock<HashMap<VisCode, &'static ModeSpecification>> = OnceLock::new();
    let map = MAP.get_or_init(|| {
        let mut map = HashMap::new();
        let mut insert = |mode: &'static ModeSpecification| map.insert(mode.vis_code, mode);
        insert(&ModeSpecification::R8BW);
        insert(&ModeSpecification::R24);
        insert(&ModeSpecification::R12BW);
        insert(&ModeSpecification::R36);
        insert(&ModeSpecification::R24BW);
        insert(&ModeSpecification::R72);
        insert(&ModeSpecification::M4);
        insert(&ModeSpecification::M3);
        insert(&ModeSpecification::M2);
        insert(&ModeSpecification::M1);
        insert(&ModeSpecification::W2180);
        insert(&ModeSpecification::S2);
        insert(&ModeSpecification::S1);
        insert(&ModeSpecification::W2120);
        insert(&ModeSpecification::SDX);
        insert(&ModeSpecification::PD50);
        insert(&ModeSpecification::PD290);
        insert(&ModeSpecification::PD120);
        insert(&ModeSpecification::PD180);
        insert(&ModeSpecification::PD240);
        insert(&ModeSpecification::PD160);
        insert(&ModeSpecification::PD90);
        insert(&ModeSpecification::P3);
        insert(&ModeSpecification::P5);
        insert(&ModeSpecification::P7);
        map
    });

    map.get(&vis_code).copied()
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("mode select error")]
pub enum ModeSelectError {
    #[error("parity error")]
    Parity,
    UnknownMode {
        vis_code: VisCode,
    },
}

pub trait SelectMode {
    fn mode_specification(&self, vis_code: VisCode) -> Result<ModeSpecification, ModeSelectError>;

    fn mode_specification_with_parity(
        &self,
        vis_code: VisCode,
        parity: bool,
    ) -> Result<ModeSpecification, ModeSelectError> {
        let expected_parity = vis_code.parity();
        tracing::debug!(?vis_code, ?expected_parity, ?parity);
        if expected_parity != parity {
            Err(ModeSelectError::Parity)
        }
        else {
            self.mode_specification(vis_code)
        }
    }
}

impl<T> SelectMode for &T
where
    T: SelectMode,
{
    #[inline]
    fn mode_specification(&self, vis_code: VisCode) -> Result<ModeSpecification, ModeSelectError> {
        (&**self).mode_specification(vis_code)
    }

    #[inline]
    fn mode_specification_with_parity(
        &self,
        vis_code: VisCode,
        parity: bool,
    ) -> Result<ModeSpecification, ModeSelectError> {
        (&**self).mode_specification_with_parity(vis_code, parity)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultModes;

impl SelectMode for DefaultModes {
    fn mode_specification(&self, vis_code: VisCode) -> Result<ModeSpecification, ModeSelectError> {
        builtin_mode_specification(vis_code)
            .copied()
            .ok_or_else(|| ModeSelectError::UnknownMode { vis_code })
    }
}

impl SelectMode for ModeSpecification {
    fn mode_specification(&self, vis_code: VisCode) -> Result<ModeSpecification, ModeSelectError> {
        if vis_code == self.vis_code {
            Ok(*self)
        }
        else {
            Err(ModeSelectError::UnknownMode { vis_code })
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::modem::sstv::modes::{
        ModeSpecification,
        VisCode,
    };

    #[test]
    fn correct_vis_codes() {
        assert_eq!(ModeSpecification::R8BW.vis_code, VisCode(0x02));
        assert_eq!(ModeSpecification::R24.vis_code, VisCode(0x04));
        assert_eq!(ModeSpecification::R12BW.vis_code, VisCode(0x06));
        assert_eq!(ModeSpecification::R36.vis_code, VisCode(0x08));
        assert_eq!(ModeSpecification::R24BW.vis_code, VisCode(0x0a));
        assert_eq!(ModeSpecification::R72.vis_code, VisCode(0x0c));
        assert_eq!(ModeSpecification::M4.vis_code, VisCode(0x20));
        assert_eq!(ModeSpecification::M3.vis_code, VisCode(0x24));
        assert_eq!(ModeSpecification::M2.vis_code, VisCode(0x28));
        assert_eq!(ModeSpecification::M1.vis_code, VisCode(0x2c));
        assert_eq!(ModeSpecification::W2180.vis_code, VisCode(0x37));
        assert_eq!(ModeSpecification::S2.vis_code, VisCode(0x38));
        assert_eq!(ModeSpecification::S1.vis_code, VisCode(0x3c));
        assert_eq!(ModeSpecification::W2120.vis_code, VisCode(0x3f));
        assert_eq!(ModeSpecification::SDX.vis_code, VisCode(0x4c));
        assert_eq!(ModeSpecification::PD50.vis_code, VisCode(0x5d));
        assert_eq!(ModeSpecification::PD290.vis_code, VisCode(0x5e));
        assert_eq!(ModeSpecification::PD120.vis_code, VisCode(0x5f));
        assert_eq!(ModeSpecification::PD180.vis_code, VisCode(0x60));
        assert_eq!(ModeSpecification::PD240.vis_code, VisCode(0x61));
        assert_eq!(ModeSpecification::PD160.vis_code, VisCode(0x62));
        assert_eq!(ModeSpecification::PD90.vis_code, VisCode(0x63));
        assert_eq!(ModeSpecification::P3.vis_code, VisCode(0x71));
        assert_eq!(ModeSpecification::P5.vis_code, VisCode(0x72));
        assert_eq!(ModeSpecification::P7.vis_code, VisCode(0x73));
    }
}
