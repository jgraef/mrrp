use crate::rtl2832u::i2c::I2cAddress;

#[derive(Clone, Copy, Debug)]
pub struct IfFilterSetting {
    pub low_q: bool,
    pub bw_1_7mhz: bool,
    pub filt_bw: u8,
    pub hpf: u8,
    pub center_frequency: f32,
}

/// Settings for the tracking filter.
#[derive(Clone, Copy, Debug)]
pub struct TrackingFilterSetting {
    /// Open drain
    ///
    /// This is related to the tracking filter, but we're not sure how. The
    /// spreadsheet mentions:
    ///
    /// > Open-D exsists on R828D, useable for switched filters
    pub open_d: OpenD,

    /// RF mux
    ///
    /// As far as it's documented, this only either bypasses the tracking
    /// filter, or not. Though it does have 2 bits, so maybe there's secret
    /// settings 😼
    pub rf_mux: RfMux,

    /// RF filter band selection
    pub rf_filt: RfFilt,

    /// Tracking low-pass filter
    ///
    /// See [`RegisterBuffer::tf_lp`].
    pub tf_lp: u8,

    /// Tracking notch filter
    ///
    /// See [`RegisterBuffer::tf_nch`].
    pub tf_nch: u8,
}

impl TrackingFilterSetting {
    pub fn bypass() -> Self {
        Self {
            open_d: OpenD::HighZ,
            rf_mux: RfMux::Bypass,
            rf_filt: RfFilt::Highest,
            tf_lp: 0,
            tf_nch: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum CrystalCapacitor {
    /// 0 F
    P0 = 0b00,
    /// 10 pF
    P10 = 0b01,
    /// 20 pF
    P20 = 0b10,
    /// 30 pF
    P30 = 0b11,
}

impl From<CrystalCapacitor> for u8 {
    #[inline(always)]
    fn from(value: CrystalCapacitor) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for CrystalCapacitor {
    type Error = u8;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(Self::P0),
            0b01 => Ok(Self::P10),
            0b10 => Ok(Self::P20),
            0b11 => Ok(Self::P30),
            _ => Err(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CrystalDrive {
    Low,
    High,
}

impl From<CrystalDrive> for bool {
    #[inline(always)]
    fn from(value: CrystalDrive) -> Self {
        match value {
            CrystalDrive::Low => false,
            CrystalDrive::High => true,
        }
    }
}

/// Crystal configuration
///
/// This contains settings for selecting the capacitors to use and the
/// `xtal_drive` setting.
///
/// We're pretty sure librtlsdr never sets this to anything but 0pF, high.
/// Though there is code to handle other configurations, but it would require
/// pre-selecting crystal settings in init, which they have commented out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CrystalConfig {
    pub capacitor: CrystalCapacitor,
    pub drive: CrystalDrive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenD {
    HighZ,
    LowZ,
}

impl From<OpenD> for bool {
    #[inline(always)]
    fn from(value: OpenD) -> Self {
        match value {
            OpenD::HighZ => false,
            OpenD::LowZ => true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum RfMux {
    TrackingFilter = 0b00,
    Bypass = 0b01,
}

impl From<RfMux> for u8 {
    #[inline(always)]
    fn from(value: RfMux) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for RfMux {
    type Error = u8;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(Self::TrackingFilter),
            0b01 => Ok(Self::Bypass),
            _ => Err(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum RfFilt {
    /// Highest band
    Highest = 0b00,
    /// RfFilt::Medium band
    Medium = 0b01,
    /// Low band
    Low = 0b10,
}

impl From<RfFilt> for u8 {
    #[inline(always)]
    fn from(value: RfFilt) -> Self {
        value as u8
    }
}

impl TryFrom<u8> for RfFilt {
    type Error = u8;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0b00 => Ok(Self::Highest),
            0b01 => Ok(Self::Medium),
            0b10 => Ok(Self::Low),
            _ => Err(value),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum VgaGain {
    /// VGA is controlled by VAGC pin
    Pin,
    Code(VgaGainCode),
}

impl VgaGain {
    pub const fn from_code(code: u8) -> Result<Self, InvalidVgaGainCode> {
        match VgaGainCode::from_code(code) {
            Ok(code) => Ok(Self::Code(code)),
            Err(error) => Err(error),
        }
    }

    pub const fn from_db(gain: f32) -> Self {
        Self::Code(VgaGainCode::from_db(gain))
    }

    pub fn as_db(&self) -> Option<f32> {
        match self {
            VgaGain::Pin => None,
            VgaGain::Code(vga_gain_code) => Some(vga_gain_code.as_db()),
        }
    }
}

impl TryFrom<u8> for VgaGain {
    type Error = InvalidVgaGainCode;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::from_code(value)
    }
}

/// VGA gain code
///
/// Can be created via [`from_code`][Self::from_code] from a number 4-bit
/// number, or via [`from_db`](Self::from_db) from a dB gain value.
///
/// The code is linear in dB, with `0b0000` being -12 dB
/// ([`MIN_DB`](Self::MIN_DB)) and `0b1111` being ([`MAX_DB`](Self::MAX_DB))
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Into)]
pub struct VgaGainCode(u8);

impl VgaGainCode {
    /// Minimum representable gain value in dB
    pub const MIN_DB: f32 = -12.0;

    /// Maximum representable gain value in dB
    pub const MAX_DB: f32 = 40.5;

    pub const fn from_code(code: u8) -> Result<Self, InvalidVgaGainCode> {
        if code & 0xf0 != 0 {
            Err(InvalidVgaGainCode { code })
        }
        else {
            Ok(Self(code))
        }
    }

    pub const fn from_db(gain: f32) -> Self {
        let code =
            ((gain - Self::MIN_DB) / (Self::MAX_DB - Self::MIN_DB) * 15.0).clamp(0.0, 15.0) as u8;
        Self(code)
    }

    pub const fn as_db(&self) -> f32 {
        self.0 as f32 / 15.0 * (Self::MAX_DB - Self::MIN_DB) + Self::MIN_DB
    }
}

impl TryFrom<u8> for VgaGainCode {
    type Error = InvalidVgaGainCode;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::from_code(value)
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid VGA gain code: {code}")]
pub struct InvalidVgaGainCode {
    pub code: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LnaGain {
    Auto,
    Code(LnaGainCode),
}

impl LnaGain {
    pub const fn from_code(code: u8) -> Result<Self, InvalidLnaGainCode> {
        match LnaGainCode::from_code(code) {
            Ok(code) => Ok(Self::Code(code)),
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Into)]
pub struct LnaGainCode(u8);

impl LnaGainCode {
    pub const fn from_code(code: u8) -> Result<Self, InvalidLnaGainCode> {
        if code & 0xf0 != 0 {
            Err(InvalidLnaGainCode { code })
        }
        else {
            Ok(Self(code))
        }
    }
}

impl TryFrom<u8> for LnaGainCode {
    type Error = InvalidLnaGainCode;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::from_code(value)
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid LNA gain code: {code}")]
pub struct InvalidLnaGainCode {
    pub code: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MixGain {
    Auto,
    Code(MixGainCode),
}

impl MixGain {
    pub const fn from_code(code: u8) -> Result<Self, InvalidMixGainCode> {
        match MixGainCode::from_code(code) {
            Ok(code) => Ok(Self::Code(code)),
            Err(error) => Err(error),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, derive_more::Into)]
pub struct MixGainCode(u8);

impl MixGainCode {
    #[inline(always)]
    pub const fn from_code(code: u8) -> Result<Self, InvalidMixGainCode> {
        if code & 0xf0 != 0 {
            Err(InvalidMixGainCode { code })
        }
        else {
            Ok(Self(code))
        }
    }
}

impl TryFrom<u8> for MixGainCode {
    type Error = InvalidMixGainCode;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::from_code(value)
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid mixer gain code: {code}")]
pub struct InvalidMixGainCode {
    pub code: u8,
}

// ----------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Model {
    R820T,
    R828D,
}

impl Model {
    pub const ALL: &[Self] = &[Self::R820T, Self::R828D];

    #[inline(always)]
    pub const fn name(&self) -> &'static str {
        match self {
            Model::R820T => "R820T",
            Model::R828D => "R828D",
        }
    }

    #[inline(always)]
    pub const fn i2c_address(&self) -> I2cAddress {
        match self {
            Model::R820T => I2cAddress::from_left_aligned(0x34),
            Model::R828D => I2cAddress::from_left_aligned(0x74),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RfInput {
    /// RF-IN on R820T, Air-In on R828D
    Air,
    /// Cable1 on R828D
    Cable1,
    /// Cable2 on R828D
    Cable2,
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid RFInputState: cable_1={cable_1:?}, cable_2={cable_2:?}")]
pub struct InvalidRfInputState {
    pub cable_1: bool,
    pub cable_2: bool,
}

pub fn voltage_to_lna_vth(voltage: f32) -> u8 {
    ((voltage - VTH_MIN) / VTH_STEP).round().clamp(0.0, 15.0) as u8
}

pub const VTH_MIN: f32 = 0.34;
pub const VTH_MAX: f32 = 1.94;
pub const VTH_STEP: f32 = (VTH_MAX - VTH_MIN) / 15.0;

pub fn lna_vth_to_voltage(vth: u8) -> f32 {
    assert!(vth < 16);
    VTH_MIN + vth as f32 * VTH_STEP
}

#[cfg(test)]
mod tests {
    use crate::tuner::r82xx::types::VgaGainCode;

    #[test]
    pub fn test_vga_code_from_db() {
        // IF VGA manual gain control - 0000=-12 dB, 1111=40.5 dB; -3.5 dB/step

        assert_eq!(VgaGainCode::from_db(-12.0), VgaGainCode(0b0000));
        assert_eq!(VgaGainCode::from_db(-8.5), VgaGainCode(0b0001));
        assert_eq!(VgaGainCode::from_db(-5.0), VgaGainCode(0b0010));
        assert_eq!(VgaGainCode::from_db(-1.5), VgaGainCode(0b0011));
        assert_eq!(VgaGainCode::from_db(2.0), VgaGainCode(0b0100));
        assert_eq!(VgaGainCode::from_db(5.5), VgaGainCode(0b0101));
        assert_eq!(VgaGainCode::from_db(9.0), VgaGainCode(0b0110));
        assert_eq!(VgaGainCode::from_db(12.5), VgaGainCode(0b0111));
        assert_eq!(VgaGainCode::from_db(16.0), VgaGainCode(0b1000));
        assert_eq!(VgaGainCode::from_db(19.5), VgaGainCode(0b1001));
        assert_eq!(VgaGainCode::from_db(23.0), VgaGainCode(0b1010));
        assert_eq!(VgaGainCode::from_db(26.5), VgaGainCode(0b1011));
        assert_eq!(VgaGainCode::from_db(30.0), VgaGainCode(0b1100));
        assert_eq!(VgaGainCode::from_db(33.5), VgaGainCode(0b1101));
        assert_eq!(VgaGainCode::from_db(37.0), VgaGainCode(0b1110));
        assert_eq!(VgaGainCode::from_db(40.5), VgaGainCode(0b1111));
    }
}
