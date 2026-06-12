use std::fmt::Debug;

use crate::rtl2832u::i2c::I2cAddress;

#[derive(Clone, Copy, Debug)]
pub struct IfFilterSetting {
    pub low_q: bool,
    pub bw_1_7mhz: bool,
    pub filt_bw: u8,
    pub hpf: u8,
    pub if_frequency: f32,
}

/// Settings for the tracking filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            CrystalDrive::Low => true,
            CrystalDrive::High => false,
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
    R828S,
}

impl Model {
    #[inline(always)]
    pub const fn name(&self) -> &'static str {
        match self {
            Model::R820T => "R820T",
            Model::R828D => "R828D",
            Model::R828S => "R828S",
        }
    }

    #[inline(always)]
    pub const fn i2c_address(&self) -> I2cAddress {
        match self {
            Model::R820T => I2cAddress::from_left_aligned(0x34),
            Model::R828D => I2cAddress::from_left_aligned(0x74),
            Model::R828S => I2cAddress::from_left_aligned(0x34),
        }
    }

    /// VCO power reference
    ///
    /// This is used when configuring the PLL. It's initialized as 2 for R820T,
    /// 1 for R828D. It also seems to be 1 for R828S (Blog V4)
    #[inline(always)]
    pub fn vco_power_ref(&self) -> u8 {
        match self {
            Model::R820T => 2,
            Model::R828D | Model::R828S => 1,
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

pub const VCO_MIN: f32 = 1770000000.0;
pub const VCO_MAX: f32 = VCO_MIN * 2.0;

/// PLL to Mixer divider number control
///
/// This seems to differ between the R820T datasheet and the implementation used
/// by librtlsdr. Specifically 0b011 doesn't actually seem to mean no division.
///
/// # TODO
///
/// I would be interesting to know which divider values work, and if no
/// divider is possible.
///
/// Without divider we could mix down signals upto 3.54 GHz. librtlsdr always
/// divides atleast by 2, that's why the RTL-SDR Blog doesn't go above 1.77 MHz
///
/// The Blog v3 uses R820T though and it also goes til 1.77 MHz. Maybe the
/// datasheet is just wrong. Maybe the description for SEL_DIV was just cut off
/// and 0b11 realy is vco_out / 16?
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct SelDiv {
    /// Register value
    pub sel_div: u8,
}

impl SelDiv {
    pub fn from_frequency(frequency: f32) -> Self {
        let div = (VCO_MIN / frequency).log2().floor().clamp(0.0, 5.0);
        let sel_div = div as u8;

        Self { sel_div }
    }

    pub fn effective_divider(&self) -> f32 {
        2.0f32.powi(i32::from(self.sel_div) + 1)
    }

    pub fn from_register_value(sel_div: u8) -> Result<Self, InvalidSelDiv> {
        if sel_div <= 5 {
            Ok(Self { sel_div })
        }
        else {
            Err(InvalidSelDiv { sel_div })
        }
    }
}

impl Debug for SelDiv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SelDiv")
            .field("register_value", &self.sel_div)
            .field("effective_divier", &self.effective_divider())
            .finish()
    }
}

impl From<SelDiv> for u8 {
    fn from(value: SelDiv) -> Self {
        value.sel_div
    }
}

impl TryFrom<u8> for SelDiv {
    type Error = InvalidSelDiv;

    #[inline(always)]
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::from_register_value(value)
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Invalid value for SEL_DIV register: 0b{sel_div:04b}")]
pub struct InvalidSelDiv {
    pub sel_div: u8,
}

/// PLL divider settings
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PllDivider {
    pub n_i2c: u8,
    pub s_i2c: u8,
    pub sdm: u16,
}

impl PllDivider {
    /// Calculate PLL divider configuration from VCO frequency
    ///
    /// `vco_power_ref` is only required for checking if a valid PLL divider
    /// config can be found. It returns `None` if that is not the case.
    ///
    /// # TODO
    ///
    /// Remove `vco_power_ref` argument. Maybe we can add a method on this that
    /// checks this condition. But we'd need to store `n_int` for that.
    pub fn from_vco_frequency(
        vco_frequency: f32,
        crystal_frequency: f32,
        vco_power_ref: u8,
    ) -> Option<Self> {
        // librtlsdr:
        //
        // uint32_t vco_fra;	/* VCO contribution by SDM (kHz) */
        // pll_ref = priv->cfg->xtal;
        // nint = vco_freq / (2 * pll_ref);
        // vco_fra = (vco_freq - 2 * pll_ref * nint) / 1000;
        // ni = (nint - 13) / 4;
        // si = nint - 4 * ni - 13;
        //
        // datasheet:
        //
        // SI2C: 2 bits
        // Ni2C: 6 bits
        //
        // Nint = 4*Ni2c+Si2c+13
        // Ndiv = (Nint + Nfra)*2
        // Nfra = SDM_IN[16] * 2^-1 + SDM_IN[15] * 2^-2 + ... + SDM_IN[2]* 2 ^-15 +
        // SDM_IN[1] * 2^-16
        //
        // the librtlsdr code for calculating sdm is pretty whack. you just need to take
        // the fractional bits and multiply by 2**16 to get them to places [15:0].
        //
        // Michele Bavaro does this [here](https://michelebavaro.blogspot.com/2014/05/gnss-carrier-phase-rtlsdr-and.html)
        // and we use that for our test.

        let n_div = 0.5 * vco_frequency / crystal_frequency;

        let n_int = n_div.floor() as u8;
        let sdm = (n_div.fract() * 65536.0) as u16;

        if n_int > (128 / vco_power_ref) - 1 {
            return None;
        }

        // todo: clarify what's going on here
        let n_i2c = (n_int - 13) / 4;
        let s_i2c = n_int - 4 * n_i2c - 13;

        Some(Self { n_i2c, s_i2c, sdm })
    }
}

#[cfg(test)]
mod tests {
    use crate::tuner::r82xx::{
        PllDivider,
        SelDiv,
        blog::BLOG_CRYSTAL_FREQ,
        types::VgaGainCode,
    };

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

    #[test]
    pub fn test_sel_div() {
        // r82xx_set_ppl: freq=30425000, div_num=05,05, vco_fine_tune=01
        // r82xx_set_ppl: freq=101625000, div_num=04,04, vco_fine_tune=01

        assert_eq!(SelDiv::from_frequency(30425000.0).sel_div, 0x05);
        assert_eq!(SelDiv::from_frequency(101625000.0).sel_div, 0x04);
    }

    #[test]
    pub fn test_pll_divider() {
        // r82xx_set_ppl: freq=30425000, vco_freq=1947200000, nint=33, vco_fra=1, ni=05,
        // si=00, n_sdm=4000
        //
        // r82xx_set_ppl: freq=101625000, vco_freq=3252000000, nint=56, vco_fra=1,
        // ni=0a, si=03, n_sdm=8000

        /// librtlsdr's SDM calculation is wrong. we test against
        /// [Michele Bavaro's](https://michelebavaro.blogspot.com/2014/05/gnss-carrier-phase-rtlsdr-and.html)
        /// code - which basically does the same as we do, but with fixed point
        /// arithmetic.
        fn michele_sdm(vco_freq: u32) -> u16 {
            let pll_ref = u64::from(BLOG_CRYSTAL_FREQ);
            let sdm = (((u64::from(vco_freq) << 16) + pll_ref) / (2 * pll_ref)) & 0xFFFF;
            u16::try_from(sdm).unwrap()
        }

        assert_eq!(
            PllDivider::from_vco_frequency(1947200000.0, BLOG_CRYSTAL_FREQ as f32, 1).unwrap(),
            PllDivider {
                n_i2c: 0x05,
                s_i2c: 0x00,
                //sdm: 0x4000,
                sdm: michele_sdm(1947200000),
            }
        );
        assert_eq!(
            PllDivider::from_vco_frequency(3252000000.0, BLOG_CRYSTAL_FREQ as f32, 1).unwrap(),
            PllDivider {
                n_i2c: 0x0a,
                s_i2c: 0x03,
                //sdm: 0x8000,
                sdm: michele_sdm(3252000000),
            }
        );
    }
}
