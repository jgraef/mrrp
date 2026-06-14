use std::{
    fmt::Debug,
    ops::{
        Index,
        IndexMut,
        Range,
    },
};

use paste::paste;

pub const NUM_REGISTERS: u8 = 0x20;

/// Initial values, starting from 0x05
#[rustfmt::skip]
pub const INITIAL: &[u8] = &[
          0x83, 0x30, 0x75, // 0x05 ..= 0x07
    0xc0, 0x40, 0xd6, 0x6c, // 0x08 ..= 0x0b
    0xf5, 0x63, 0x75, 0x68, // 0x0c ..= 0x0f
    0x6c, 0x83, 0x80, 0x00, // 0x10 ..= 0x13
    0x0f, 0x00, 0xc0, 0x30, // 0x14 ..= 0x17
    0x48, 0xcc, 0x60, 0x00, // 0x18 ..= 0x1b
    0x54, 0xae, 0x4a, 0xc0, // 0x1c ..= 0x1f
];

/// Buffered register state of the R82xxx
///
/// This doesn't perform any actual reads or writes, but caches data locally. To
/// actually fetch registers from the tuner use [`Transaction::read`]. To write
/// all changed registers to the tuner use [`Transaction::flush`].
///
/// # Initialization
///
/// `librtlsdr` initializes ot as follows:
///
/// ```c
/// /* Those initial values start from REG_SHADOW_START (5) */
/// static const uint8_t r82xx_init_array[NUM_REGS] = {
/// 	0x83, 0x30, 0x75,			/* 05 to 07 */
/// 	0xc0, 0x40, 0xd6, 0x6c,			/* 08 to 0b */
/// 	0xf5, 0x63, 0x75, 0x68,			/* 0c to 0f */
/// 	0x6c, 0x83, 0x80, 0x00,			/* 10 to 13 */
/// 	0x0f, 0x00, 0xc0, 0x30,			/* 14 to 17 */
/// 	0x48, 0xcc, 0x60, 0x00,			/* 18 to 1b */
/// 	0x54, 0xae, 0x4a, 0xc0			/* 1c to 1f */
/// };
/// ```
///
/// We're doing this as well, but it's a bit hard to know what these do. We need
/// to replace these with actual calls to setters.
///
/// # Dump
///
/// Dump from testing of first 0x10 registers (from actual device):
///
/// ```plain
/// │00000000│ 69 01 01 ff ab 05 8d 5c ┊ 02 03 6c d6 ac ca ae 16 │i••××•×\┊••l××××•│
/// ```
#[derive(Clone, Copy, Default)]
pub struct RegisterBuffer {
    state: [u8; NUM_REGISTERS as usize],
    modified: Modified,
}

impl RegisterBuffer {
    #[inline(always)]
    pub fn from_state(state: [u8; NUM_REGISTERS as usize]) -> Self {
        Self {
            state,
            modified: Modified::default(),
        }
    }

    #[inline(always)]
    pub fn set_no_dirty(&mut self, index: u8, value: u8) {
        self.state[usize::from(index)] = value;
    }

    #[inline(always)]
    pub fn is_modified(&self, register: u8) -> bool {
        self.modified.contains(register)
    }

    #[inline(always)]
    pub fn state(&self) -> &[u8; NUM_REGISTERS as usize] {
        &self.state
    }

    #[inline(always)]
    pub fn modified(&self) -> Modified {
        self.modified
    }

    #[inline(always)]
    pub fn clear_modified(&mut self) {
        self.modified = Default::default();
    }
}

impl Index<u8> for RegisterBuffer {
    type Output = u8;

    #[inline(always)]
    fn index(&self, index: u8) -> &Self::Output {
        &self.state[usize::from(index)]
    }
}

impl Index<Range<u8>> for RegisterBuffer {
    type Output = [u8];

    #[inline(always)]
    fn index(&self, index: Range<u8>) -> &Self::Output {
        &self.state[usize::from(index.start)..usize::from(index.end)]
    }
}

impl IndexMut<u8> for RegisterBuffer {
    #[inline(always)]
    fn index_mut(&mut self, index: u8) -> &mut Self::Output {
        self.modified.set_index(index);
        &mut self.state[usize::from(index)]
    }
}

impl IndexMut<Range<u8>> for RegisterBuffer {
    #[inline(always)]
    fn index_mut(&mut self, index: Range<u8>) -> &mut Self::Output {
        self.modified.set_range(index.clone());
        &mut self.state[usize::from(index.start)..usize::from(index.end)]
    }
}

/// Register 0x15 and 0x16
impl RegisterBuffer {
    #[inline(always)]
    pub fn sdm_in(&self) -> u16 {
        u16::from_le_bytes(self[0x15..0x17].try_into().unwrap())
    }

    /// PLL fractional divider number input `SDM[16:1]`
    ///
    /// Register 0x15 = `SDM[8:1]`
    ///
    /// Register 0x16 = `SDM[16:9]`
    ///
    /// # TODO
    ///
    /// Decipher this:
    ///
    /// ```plain
    /// PLL fractional divider number input SDM[16:1]
    /// Nfra=SDM_IN[16]*2^-1+SDM_IN[15]*2^-2+E+SDM_IN[2]
    /// *2^-15+SDM_IN[1]*2^-16
    /// ```
    #[inline(always)]
    pub fn set_sdm_in(&mut self, value: u16) {
        self[0x15..0x17].copy_from_slice(&value.to_le_bytes());
    }
}

#[inline(always)]
fn range_mask(start: u8, end: u8) -> u32 {
    let end_mask = if end == 32 { u32::MAX } else { (1 << end) - 1 };
    let start_mask = (1 << start) - 1;

    end_mask ^ start_mask
}

/// Set of modified registers
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Modified(u32);

impl Modified {
    #[inline(always)]
    pub fn contains(&self, index: u8) -> bool {
        self.0 & (1 << index) != 0
    }

    #[inline(always)]
    pub fn set_index(&mut self, index: u8) {
        self.0 |= 1 << index;
    }

    #[inline(always)]
    pub fn set_range(&mut self, range: Range<u8>) {
        self.0 |= range_mask(range.start, range.end);
    }

    #[inline(always)]
    pub fn iter(&self) -> impl Iterator<Item = u8> {
        (0..NUM_REGISTERS).filter(|index| self.contains(*index))
    }

    pub fn any(&self) -> bool {
        self.0 != 0
    }
}

impl Debug for Modified {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut list = f.debug_list();

        for i in self.iter() {
            list.entry(&FormatRegisterAddress(i));
        }

        list.finish()
    }
}

struct FormatRegisterAddress(u8);

impl Debug for FormatRegisterAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:02x}", self.0)
    }
}

macro_rules! registers {
    {$(
        $(#[$register_meta:meta])*
        $address:literal: {$(
            $(#[$field_meta:meta])*
            $name:ident: [$msb:literal $(: $lsb:literal)?],
        )*};
    )*} => {
        $(
            $(#[$register_meta])*
            #[doc = concat!("\n\nRegister ", stringify!($address))]
            impl RegisterBuffer {
                $(
                    registers!(@generate_impl($name, $address, [$msb, $($lsb)?], [$($field_meta),*]));
                )*
            }
        )*


        impl Debug for RegisterBuffer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                let mut s = f.debug_struct("RegisterBuffer");
                s.field("_state", &self.state);
                $($(
                    s.field(stringify!($name), &self.$name());
                )*)*
                s.finish()
            }
        }

        #[cfg(test)]
        #[test]
        fn no_fields_overlap() {
            let mut registers = std::collections::HashMap::<(u8, u8), &'static str>::default();
            $($({
                registers!(@generate_test(registers, $name, $address, [$msb, $($lsb)?], [$($field_meta),*]));
            })*)*
        }
    };
    (@generate_impl($name:ident, $address:literal, [$bit:literal,], [$($meta:meta),*])) => {
        $(#[$meta])*
        #[doc = concat!("\n\nRegister ", stringify!($address), ", Bit ", $bit)]
        #[inline(always)]
        pub fn $name(&self)  -> bool {
            ::bitfield::Bit::bit(&self[$address], $bit)
        }

        paste! {
            $(#[$meta])*
            #[doc = concat!("\n\nRegister ", stringify!($address), ", Bit ", $bit)]
            #[inline(always)]
            pub fn [<set_ $name>](&mut self, value: bool) {
                ::bitfield::BitMut::set_bit(&mut self[$address], $bit, value);
            }
        }
    };
    (@generate_impl($name:ident, $address:literal, [$msb:literal,$lsb:literal], [$($meta:meta),*])) => {
        $(#[$meta])*
        #[doc = concat!("\n\nRegister ", stringify!($address), ", Bits ", $msb, ":", $lsb)]
        #[inline(always)]
        pub fn $name(&self)  -> u8 {
            ::bitfield::BitRange::<u8>::bit_range(&self[$address], $msb, $lsb)
        }

        paste! {
            $(#[$meta])*
            #[doc = concat!("\n\nRegister ", stringify!($address), ", Bits ", $msb, ":", $lsb)]
            #[inline(always)]
            pub fn [<set_ $name>](&mut self, value: u8) {
                ::bitfield::BitRangeMut::<u8>::set_bit_range(&mut self[$address], $msb, $lsb, value);
            }
        }
    };
    (@generate_test($registers:expr, $name:ident, $address:literal, [$bit:literal,], [$($meta:meta),*])) => {
        if let Some(used_by) = $registers.get(&($address, $bit)) {
            panic!("Field {} also used by {used_by} at bit {}", stringify!($name), $bit);
        }
        $registers.insert(($address, $bit), stringify!($name));
    };
    (@generate_test($registers:expr, $name:ident, $address:literal, [$msb:literal,$lsb:literal], [$($meta:meta),*])) => {
        assert!($msb > $lsb, "Invalid bitrange for {} 0x{:02x} [{}:{}]", stringify!($name), $address, $msb, $lsb);

        for bit in $lsb..=$msb {
            if let Some(used_by) = $registers.get(&($address, bit)) {
                panic!("Field {} also used by {used_by} at bit {bit} in register 0x{:02x}", stringify!($name), $address);
            }
            $registers.insert(($address, bit), stringify!($name));
        }
    };
}

registers! {
    0x00: {
        /// For probing - always 0x69
        ///
        /// Read-only
        test: [7:0],
    };
    0x01: {
        /// ADC on output from PDET3
        ///
        /// and possibly other options?
        ///
        /// Read-only
        unk_adc_output: [5:0],
    };
    0x02: {
        unk_autotune_done: [7],
        pll_lock: [6],
        /// VCO indicator
        ///
        /// Read-only
        ///
        /// TODO: split?
        ///
        /// ```plain
        /// vco_core: [5],
        /// vco_band: [4:0],
        /// ```
        vco_indicator: [5:0],
    };
    0x03: {
        /// RF indicator
        ///
        /// Read-only
        ///
        /// TODO: Split?
        ///
        /// ```plain
        /// mixer_gain: [7:4],
        /// lna_gain: [0:3],
        /// ```
        rf_indicator: [7:0],
    };
    0x04: {
        /// VCO fine tune
        ///
        /// Not in datasheet, but in librtlsdr.
        ///
        /// `vco_fine_tune = (data[4] & 0x30) >> 4;`
        ///
        /// `vco_power_ref` is set to 1 for any R828D, or any Blog V4L.
        ///
        /// Controls `div_num` (in 0x10):
        ///
        /// ```c
        /// if (vco_fine_tune > vco_power_ref)
        ///     div_num = div_num - 1;
        /// else if (vco_fine_tune < vco_power_ref)
        ///     div_num = div_num + 1;
        /// ```
        unk_vco_fine_tune: [5:4],
        /// Filter calibration code
        ///
        /// Not in datasheet, but in librtlsdr.
        ///
        /// See `r82xx_set_tv_standard`.
        unk_fil_cal_code: [3:0],
    };
    0x05: {
        /// Loop through - 0=on, 1=off
        pwd_lt: [7],
        /// Not in datasheet, but used by librtlsdr
        ///
        /// Referred to as `cable_1_in`. It's 1=on if VHF band is used. In the datasheet this is fixed to 0.
        ///
        /// ```c
        /// /* activate cable 1 (VHF input) */
        /// cable_1_in = (band == VHF) ? 0x40 : 0x00;
        /// rc = r82xx_write_reg_mask(priv, 0x05, cable_1_in, 0x40);
        /// ```
        ///
        /// This might be the second LNA that is mentioned for the R828D.
        unk_cable_1_in: [6],
        /// LNA 1 power control - 0=on, 1=off
        ///
        /// Also referred to as `air_in` in librtlsdr. It's 0=on if UHF band is used.
        /// We think this doesn't turn on `air_in`, but rather just disables the LNA, if `cable_1_in` or `cable_2_in` are used.
        ///
        /// ```c
        /// /* activate air_in (UHF input) */
        /// air_in = (band == UHF) ? 0x00 : 0x20;
        /// rc = r82xx_write_reg_mask(priv, 0x05, air_in, 0x20);
        /// ```
        pwd_lna1: [5],
        /// LNA gain mode switch - 0=auto, 1=manual
        lna_gain_mode: [4],
        /// LNA manual gain control - 15(max) .. 0(min)
        lna_gain: [3:0],
    };
    0x06: {
        /// Power detector 1 - 0=on, 1=off
        pwd_pdet1: [7],
        /// Power detector 3 - 0=on, 1=off
        ///
        /// The spreadsheet calls this `PWD_PDET2`. I think the datasheet is right, but ya never know.
        pwd_pdet3: [6],
        /// Filter gain 3dB - 0=0dB, 1=+3dB
        filt_3db: [5],
        /// Not in datasheet, but used by librtlsdr.
        ///
        /// ```c
        /// filt_gain = 0x30;	/* +3db, 6mhz on, 0b0011_0000 */
        /// /* Set filt_3dB, V6MHz */
        /// rc = r82xx_write_reg_mask(priv, 0x06, filt_gain, 0x30);
        /// ```
        unk_v6mhz: [4],
        /// Not in datasheet, but used by librtlsdr.
        ///
        /// Referred to as `cable_2_in`. It's 1=on if HF band is used. In the datasheet this is fixed to 0.
        ///
        /// ```c
        /// /* activate cable 2 (HF input) */
        /// cable_2_in = (band == HF) ? 0x08 : 0x00;
        /// rc = r82xx_write_reg_mask(priv, 0x06, cable_2_in, 0x08);
        /// ```
        unk_cable_2_in: [3],
        /// LNA power control - 000(max) .. 111(min)
        pw_lna: [2:0],
    };
    0x07: {
        /// Undocumented
        ///
        /// ```c
        /// img_r = 0x00;		/* image negative */
        /// /* Set Img_R */
        /// rc = r82xx_write_reg_mask(priv, 0x07, img_r, 0x80);
        /// ```
        unk_img_r: [7],
        /// Mixer power - 0=on, 1=off
        pwd_mix: [6],
        /// Mixer current control - 0=max current, 1=normal current
        pw0_mix: [5],
        /// Mixer gain mode - 0=manual, 1=auto
        mixgain_mode: [4],
        /// Mixer manual gain control - 0000(min) .. 1111(max)
        mix_gain: [3:0],
    };
    0x08: {
        /// Mixer buffer power - 0=off, 1=on
        pwd_amp: [7],
        /// Mixer buffer current setting - 0=high, 1=low
        pw0_amp: [6],
        /// Image gain adjustment - 0(min) .. 63(max)
        imr_g: [5:0],
    };
    0x09: {
        /// IF Filter power - 0=on, 1=off
        pwd_iffilt: [7],
        /// IF Filter current - 0=high, 1=low
        pw1_iffilt: [6],
        /// Image phase adjustment - 0(min) .. 63(max)
        imr_p: [5:0],
    };
    0x0a: {
        /// Filter power - 0=off, 1=on
        pwd_filt: [7],
        /// Filter power control - 00=high .. 11=low
        pw_filt: [6:5],
        /// Undocumented, but in librtlsdr.
        ///
        /// This is always set to 1 in `r82xx_set_tv_standard`:
        ///
        /// ```c
        /// filt_q = 0x10;		/* r10[4]:low q(1'b1) */
        /// ```
        ///
        /// Set also by `r82xx_set_bandwidth`
        ///
        /// Spreadsheet: "Low-Q: should give less sharp filter edges"
        unk_filt_q: [4],
        /// Filter bandwidth manual fine tune - 0000=widest, 1111=narrowest
        ///
        /// This can be calibrated. See field `unk_fil_cal_code` and librtlsdr function `r82xx_set_tv_standard`.
        filt_code: [3:0],
    };
    0x0b: {
        /// Undocumented
        ///
        /// Seems to enable filtering with < 1.7 MHz bandwidth
        unk_bw_1_7mhz: [7],
        /// Filter bandwidth manual coarse tunnel - 00=widest, 10 or 01=middle, 11=narrowest
        filt_bw: [6:5],
        /// Undocumented
        ///
        /// Is this the bit that triggers calibration?
        unk_cal_trig: [4],
        /// High pass filter corner control - 0000=high, 1111=low
        hpf: [3:0],
    };
    0x0c: {
        /// Undocumented
        ///
        /// In spreadsheet: 0=on, 1=off
        unk_adc_enable: [7],
        /// VGA power control - 0=off, 1=on
        pwd_vga: [6],
        /// VGA power setting (not in datasheet)
        ///
        /// If it's like the other power settings 0=high, 1=low. This is confirmed because librtlsdr sets this to 1 when going into standby.
        unk_pw0_vga: [5],
        /// VGA gain manual / pin selector - 1=IF vga gain controlled by vagc pin, 0=IF vga gain controlled by `vga_code[5:0]`
        vga_mode: [4],
        /// IF VGA manual gain control - 0000=-12 dB, 1111=40.5 dB; -3.5 dB/step
        vga_code: [3:0],
    };
    0x0d: {
        /// LNA AGC power detector voltage threshold high setting - 1111=1.94V, 0000=0.34V; ~0.1V/step
        lnavth_h: [7:4],
        /// LNA AGC power detector voltage threshold low setting - 1111=1.94V, 0000=0.34V; ~0.1V/step
        lnavth_l: [3:0],
    };
    0x0e: {
        /// Mixer AGC power detector voltage threshold high setting - 1111=1.94V, 0000=0.34V; ~0.1V/step
        mixvth_h: [7:4],
        /// Mixer AGC power detector voltage threshold low setting - 1111=1.94V, 0000=0.34V; ~0.1V/step
        mixvth_l: [3:0],
    };
    0x0f: {
        /// Not in datasheet.
        ///
        /// In the datasheet this is always 0
        ///
        /// librtlsdr:
        ///
        /// ```c
        /// flt_ext_widest = 0x00;	/* r15[7]: flt_ext_wide off */
        /// rc = r82xx_write_reg_mask(priv, 0x0f, flt_ext_widest, 0x80);
        /// ```
        ///
        /// so, 1=on, 0=off
        unk_filt_ext_widest: [7],
        /// Clock out pin control - 0=on, 1=off (clk output)
        clk_out_enb: [4],
        /// AGC clk control - 0=on, 1=off (internal agc clock)
        clk_agc_enb: [1],
    };
    0x10: {
        /// PLL to Mixer divider number control
        ///
        /// - 000: mixer in = vco out / 2
        /// - 001: mixer in = vco out / 4
        /// - 010: mixer in = vco out / 8
        /// - 011: mixer in = vco out
        sel_div: [7:5],
        /// PLL Reference frequency divider
        ///
        /// - 0: fref=xtal_freq
        /// - 1: fref=xtal_freq / 2 (for Xtal > 24 MHz)
        ///
        ref_div2: [4],
        /// Unknown related to capx
        ///
        /// high=0, low=1
        ///
        /// This is set by librtlsdr together with capx via `r82xx_xtal_cap_value` enum.
        ///
        /// ```plain
        /// XTAL_LOW_CAP_30P: 0x0b: 0b0_1011
        /// XTAL_LOW_CAP_20P: 0x02: 0b0_0010
        /// XTAL_LOW_CAP_10P: 0x01: 0b0_0001
        /// XTAL_LOW_CAP_0P:  0x00: 0b0_0000
        /// XTAL_HIGH_CAP_0P: 0x10: 0b1_0000
        /// ```
        ///
        /// But this mis masked with `0b1011` and ored with `0b1000` if it's anything but `XTAL_HIGH_CAP_0P`.
        ///
        /// They never set anything other than `XTAL_HIGH_CAP_0P` anyway. Although there is an unused function to test for the best values.
        unk_drive: [3],
        /// Unknown - always 1
        unk_cap: [2],
        /// Internal xtal cap setting - 00=no cap, 01=10pF, 10=20pF, 11=30pF
        capx: [1:0],
    };
    0x11: {
        /// PLL analog low drop out regulator switch - 00=off, 01=2.1V, 10=2.0V, 11=1.9V
        pw_ldo_a: [7:6],
        /// Not in datasheet
        ///
        /// In librtlsdr:
        ///
        /// ```c
        /// cp_cur = 0x38;		/* 111, auto */
        /// rc = r82xx_write_reg_mask(priv, 0x11, cp_cur, 0x38);
        /// mask = 0b0011_1000
        /// ```
        ///
        /// CP pin - PLL charge pump?
        ///
        /// Set to 0b000 for standby. So the 0b000=low/off, 0b111=highest maybe?
        ///
        /// The spreadsheet lists this as 0=on
        unk_cp_cur: [5:3],

        /// Undocumented
        ///
        /// No idea what this does.
        ///
        /// It's initialized as 0b11 and set to 0b11 on standby. Spreadsheet just names it.
        unk_bias_hf: [1:0],
    };
    0x12: {
        /// Not in datasheet
        ///
        /// 0b000 is max?
        ///
        /// ```c
        /// /* set VCO current = 100 */
        /// /* rc = r82xx_write_reg_mask(priv, 0x12, 0x80, 0xe0); */
        /// /* RTL-SDR Blog Modification: Set VCO current to MAX */
        /// rc = r82xx_write_reg_mask(priv, 0x12, 0x06, 0xff);
        /// ```
        unk_vco_current: [7:5],
        /// Not in datasheet
        ///
        /// In the datasheet this bit is fixed to 0
        ///
        /// In the spreadsheet this is called `DIS_DITHER`, grouped with PLL, but now on-state info.
        ///
        /// In librtlsdr this is set to `false` in `r82xx_set_pll`.
        unk_dis_dither: [4],
        /// In datasheet, but no description
        ///
        /// Likely power for SDM.
        ///
        /// In spreadsheet: 0=on, 1=off
        pw_sdm: [3],
        /// Not in datasheet
        ///
        /// CP pin - PLL charge pump?
        unk_cp_offset: [2:1],
        /// Not in datasheet
        ///
        /// CP pin - PLL charge pump?
        unk_cp_0406: [0],
    };
    0x13: {
        /// PLL auto_tune clk
        ///
        /// 0=on, 1=off
        ///
        /// Undocumented, but used by librtlsdr
        ///
        /// See `r82xx_xtal_check`
        unk_auto_tune: [7],
        /// Undocumented
        ///
        /// 1=on, 0=off
        unk_band_force: [6],
        /// Undocumented, but used by librtlsdr.
        ///
        /// This is usually set to 0x31. In `r82xx_xtal_check` it is set to 0x3f.
        unk_vco_band: [5:0],
    };
    0x14: {
        /// PLL integer divider number input Si2c
        ///
        /// Nint = 4 * Ni2c + Si2c + 13
        /// PLL divider number Ndiv = (Nint + Nfra) * 2
        s_i2c:[7:6],
        /// PLL integer divider number input Ni2c
        n_i2c:[5:0],
    };
    0x17: {
        /// PLL digital low drop out regulator supply current switch:
        ///
        /// - 00: 1.8V, 8mA
        /// - 01: 1.8V, 4mA
        /// - 10: 2.0V, 8mA
        /// - 11: off
        pw_ldo_d: [7:6],
        /// Not in datasheet
        ///
        /// in librtlsdr:
        ///
        /// ```c
        /// div_buf_cur = 0x20;	/* 10, 200u */
        /// div_buf_cur = 0x30;	/* 11, 150u */
        /// /* RTL-SDR Blog Hack. Improve L-band performance by setting PLL drop out to 2.0v */
        /// div_buf_cur = 0xa0;  // 0b10, because it's masked
        /// rc = r82xx_write_reg_mask(priv, 0x17, div_buf_cur, 0x30);
        /// ```
        ///
        /// Set to 0b11 for standby.
        ///
        /// Looks like:
        ///  - 0b11 = 150 uA
        ///  - 0b10 = 200 uA
        ///  - 0b01 = 250 uA (extrapolated)
        ///  - 0b00 = 300 uA (extrapolated)
        unk_div_buf_cur: [5:4],
        /// Open drain - 0=high-z, 1=low-z
        open_d: [3],
        /// Not in datasheet
        ///
        /// librtlsdr: initializes to 0b00, sets to 0b10 for standby.
        ///
        /// spreadsheet labels this "1 0 pw_IQ" in power group.
        unk_pw_iq: [2:1],
        /// Not in datasheet
        ///
        /// librtlsdr: initializes to 0, sets to 0 for standby.
        ///
        /// spreadsheet labels this "1 0 pw_IQ" in power group. Also labels it as 0=on
        unk_pwd_iq: [0],
    };
    0x19: {
        /// RF filter power - 0=off, 1=on
        pwd_rffilt: [7],
        /// RF poly filter current
        ///
        /// Not in datasheet - there it's always `0b00`.
        ///
        /// In librtlsdr `r82xx_set_tv_standard`:
        ///
        /// ```c
        /// polyfil_cur = 0x60;	/* r25[6:5]:min */
        /// /* RF poly filter current */
        /// rc = r82xx_write_reg_mask(priv, 0x19, polyfil_cur, 0x60);
        /// ```
        ///
        /// So is `0b11` minimum then?
        ///
        /// `r82xx_standby` sets this to `0b00`:
        ///
        /// ```c
        /// rc = r82xx_write_reg(priv, 0x19, 0x0c);
        /// ```
        ///
        /// Spreadsheet splits this into "ring_cp_current" and "POLYFIL_CUR"
        unk_rf_poly_filter_current: [6:5],
        /// Switch agc_pin
        ///
        /// - 0: agc = agc_in
        /// - 1: agc = agc_in2
        sw_agc: [4],
    };
    0x1a: {
        /// Tracking filter switch - 00=TF on, 01=bypass
        ///
        /// In librtlsdr:
        ///
        /// ```c
        /// /* .rf_mux_ploy = */	0x02,	/* R26[7:6]=0 (LPF)  R26[1:0]=2 (low) */
        /// /* .rf_mux_ploy = */	0x41,	/* R26[7:6]=1 (bypass)  R26[1:0]=1 (middle) */
        /// ```
        ///
        /// So 0b01 is a LPF.
        rfmux: [7:6],
        /// AGC clock (not in datasheet)
        ///
        /// In librtlsdr:
        ///
        /// ```c
        /// /* agc clk 250hz */
        /// rc = r82xx_write_reg_mask(priv, 0x1a, 0x30, 0x30);
        /// /* agc clk 60hz */
        /// rc = r82xx_write_reg_mask(priv, 0x1a, 0x20, 0x30);
        /// /* agc clk 1Khz, external det1 cap 1u */
        /// rc = r82xx_write_reg_mask(priv, 0x1a, 0x00, 0x30);
        /// ```
        ///
        /// In datasheet this is fixed as 0b10
        ///
        /// - 0b00 - 1 kHz
        /// - 0b10 - 60 Hz
        /// - 0b11 = 250 Hz
        unk_agc_clk: [5:4],
        /// PLL auto tune clock rate - 00=128kHz, 01=32kHz, 10=8kHz
        pll_auto_clk: [3:2],
        /// RF filter band selection, 00=highest, 01=medium, 10=low
        rffilt: [1:0],
    };
    0x1b: {
        /// 0000 highest corner for LPNF; 1111 lowest corner for LPNF
        tf_nch: [7:4],
        /// 0000 highest corner for LPF; 1111 lowest corner for LPF
        tf_lp: [3:0],
    };
    0x1c: {
        /// Power detector 3 TOP(take off point) control - 0=highest .. 15=lowest
        pdet3_gain: [7:4],
        unk_lna_top_p1: [3],
        /// Not in datasheet
        ///
        /// in librtlsdr:
        ///
        /// ```c
        /// // /* 0: normal mode */
        //  rc = r82xx_write_reg_mask(priv, 0x1c, 0, 0x04);
        /// ```
        unk_discharge_mode: [2],
        /// 0=rf, 1=ring
        unk_mixer_src: [1],
        unk_vco_out: [0],
    };
    0x1d: {
        /// Undocumented, but set by librtlsdr
        ///
        /// ```c
        /// lna_top = 0xe5;		/* detect bw 3, lna top:4, predet top:2 */
        /// rc = r82xx_write_reg_mask(priv, 0x1d, lna_top, 0xc7);
        /// mask    = 0b1100_0111
        /// lna_top = 0b1110_0101
        /// ```
        ///
        /// Reasonable that they split it this way `0b11, 100, 101`, which corresponds to datasheet.
        ///
        /// So "detect bw" is this undocumented field.
        /// "lna top" is `pdet1_gain`, but it's masked out when they set it.
        /// And "predet top" is `pdet2_gain`, but they actually set it to 5.
        ///
        unk_detect_bw: [7:6],
        /// Power detector 1 TOP(take off point) control - 0=highest .. 15=lowest
        pdet1_gain: [5:3],
        /// Power detector 2 TOP(take off point) control - 0=highest .. 15=lowest
        pdet2_gain: [2:0],
    };
    0x1e: {
        /// Filter extension under weak signal - 0=off, 1=on
        ///
        /// Not mentioned in the datasheet's register matrix, but in the descriptions. In the matrix it's always 1.
        filter_ext: [6],
        /// Undocumented
        ///
        /// This is also part of [`pdet_clk`](Self::pdet_clk), but librtlsdr uses it separately and the spreadsheet lists is separately.
        ///
        /// In `r82xx_set_tv_standard`:
        ///
        /// ```c
        /// ext_enable = 0x60;	/* r30[6]=1 ext enable; r30[5]:1 ext at lna max-1 */
        /// ```
        ext_enable: [5],
        /// Power detector timing control - 111111=max, 000000=min
        pdet_clk: [4:0],
    };
    0x1f: {
        /// Not in datasheet
        ///
        /// In librtlsdr:
        ///
        /// ```c
        /// lt_att = 0x00;		/* r31[7], lt att enable */
        /// rc = r82xx_write_reg_mask(priv, 0x1f, lt_att, 0x80);
        /// ```
        ///
        /// So, 0=on, 1=off?
        ///
        /// In the datasheet this is always 1
        unk_lt_att: [7],
    };
}
