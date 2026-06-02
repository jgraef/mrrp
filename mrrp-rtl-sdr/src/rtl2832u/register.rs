use std::fmt::Debug;

use bitfield::bitfield;
use nusb::transfer::{
    ControlIn,
    ControlOut,
    ControlType,
    Recipient,
};

use crate::rtl2832u::I2cAddress;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Block {
    Demod { page: u8 },
    Usb,
    System,
    Tuner,
    Rom,
    I2c,
}

impl Block {
    /// Returns base address where applicable.
    ///
    /// Some blocks (usb and system) have a base address. This is already taken
    /// care of when you get a [`Register`], but might be interesting to know.
    pub fn base_address(&self) -> Option<u16> {
        match self {
            Block::Demod { page: _ } => None,
            Block::Usb => Some(0x2000),
            Block::System => Some(0x3000),
            Block::Tuner => todo!("not in datasheet"),
            Block::Rom => None,
            Block::I2c => None,
        }
    }

    #[track_caller]
    pub fn with_address(&self, address: u16) -> Register {
        match self {
            Block::Demod { page } => {
                if *page > 4 {
                    panic!("Invalid demod page: {page}");
                }

                Register::Demod {
                    page: *page,
                    address: address
                        .try_into()
                        .unwrap_or_else(|_| panic!("Invalid demod address: 0x{address:04x}")),
                }
            }
            Block::Usb => Register::Usb { address },
            Block::System => Register::System { address },
            Block::Tuner => Register::Tuner { address },
            Block::Rom => Register::Rom { address },
            Block::I2c => {
                panic!("Not possible to create an I2C register from block and address");
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Register {
    Demod { page: u8, address: u8 },
    Usb { address: u16 },
    System { address: u16 },
    Tuner { address: u16 },
    Rom { address: u16 },
    I2c { i2c_address: I2cAddress },
}

impl Register {
    pub fn block(&self) -> Block {
        match self {
            Register::Demod { page, address: _ } => Block::Demod { page: *page },
            Register::Usb { address: _ } => Block::Usb,
            Register::System { address: _ } => Block::System,
            Register::Tuner { address: _ } => Block::Tuner,
            Register::Rom { address: _ } => Block::Rom,
            Register::I2c { i2c_address: _ } => Block::I2c,
        }
    }

    pub fn address(&self) -> u16 {
        match self {
            Register::Demod { page: _, address } => (*address).into(),
            Register::Usb { address } => *address,
            Register::System { address } => *address,
            Register::Tuner { address } => *address,
            Register::Rom { address } => *address,
            Register::I2c {
                i2c_address: address,
            } => address.0.into(),
        }
    }

    pub fn w_value(&self) -> u16 {
        match self {
            Register::Demod { page: _, address } => {
                // This is not documented in the datasheet. It mentions under vendor commands
                // that this should just be the "Reg's offset".
                //
                // I tried reading IIC_repeat, but got an "endpoint stalled" error.
                //
                // `rtlsdr_demod_read_reg` in `librtlsdr` transforms this address as follows
                //
                // also couldn't find anything about this in the linux drivers
                //
                // but after getting the parenthesis right, it works :3

                (u16::from(*address) << 8) | 0x20
            }
            Register::Usb { address } => *address,
            Register::System { address } => *address,
            Register::Tuner { address } => *address,
            Register::Rom { address } => *address,
            Register::I2c {
                i2c_address: address,
            } => address.0.into(),
        }
    }

    pub fn w_index(&self, write: bool) -> u16 {
        let mut w_index = match self {
            Register::Demod { page, address: _ } => {
                assert!(*page <= 4);
                u16::from(*page)
            }
            Register::Usb { address: _ } => 0x0100,
            Register::System { address: _ } => 0x0200,
            Register::Tuner { address: _ } => 0x0300,
            Register::Rom { address: _ } => 0x0400,
            Register::I2c { i2c_address: _ } => 0x0600,
        };

        if write {
            w_index |= 0x10;
        }

        w_index
    }

    pub fn control_in(&self, length: u16) -> ControlIn {
        ControlIn {
            // bmRequestType, vendor command
            control_type: ControlType::Vendor,
            // bmRequestType, endpoint
            recipient: Recipient::Endpoint,
            // bRequest
            request: 0,
            // wValue
            value: self.w_value(),
            // wIndex
            index: self.w_index(false),
            // wLength
            length,
        }
    }

    pub fn control_out<'a>(&self, data: &'a [u8]) -> ControlOut<'a> {
        ControlOut {
            // bmRequestType, vendor command
            control_type: ControlType::Vendor,
            // bmRequestType, endpoint
            recipient: Recipient::Endpoint,
            // bRequest
            request: 0,
            // wValue
            value: self.w_value(),
            // wIndex
            index: self.w_index(true),
            // wLength implicit
            data,
        }
    }

    pub const fn demod(page: u8, address: u8) -> Self {
        Self::Demod { page, address }
    }

    pub const fn usb(address: u16) -> Self {
        Self::Usb { address }
    }

    pub const fn sys(address: u16) -> Self {
        Self::System { address }
    }
}

pub trait RegisterValue: Debug {
    const ADDRESS: Register;
    type Bits: Bits;

    fn from_bits(bits: Self::Bits) -> Self;
    fn as_bits(&self) -> Self::Bits;
}

pub trait Bits {
    type Bytes: AsRef<[u8]>;
    const LENGTH: u16;

    fn from_bytes(bytes: &[u8]) -> Self;
    fn into_bytes(&self) -> Self::Bytes;
}

impl Bits for u8 {
    type Bytes = [u8; 1];

    const LENGTH: u16 = 1;

    #[inline(always)]
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes[0]
    }

    #[inline(always)]
    fn into_bytes(&self) -> Self::Bytes {
        [*self]
    }
}

macro_rules! impl_bits {
    ($ty:ty, $bytes:expr) => {
        impl Bits for $ty {
            type Bytes = [u8; $bytes];

            const LENGTH: u16 = $bytes;

            #[inline(always)]
            fn from_bytes(bytes: &[u8]) -> Self {
                Self::from_le_bytes(bytes.try_into().unwrap())
            }

            #[inline(always)]
            fn into_bytes(&self) -> Self::Bytes {
                self.to_le_bytes()
            }
        }
    };
}

impl_bits!(u16, 2);
impl_bits!(u32, 4);
impl_bits!(u64, 8);

impl<const N: usize> Bits for [u8; N] {
    type Bytes = Self;

    const LENGTH: u16 = const { N as u16 };

    #[inline(always)]
    fn from_bytes(bytes: &[u8]) -> Self {
        bytes.try_into().unwrap()
    }

    #[inline(always)]
    fn into_bytes(&self) -> Self::Bytes {
        *self
    }
}

pub trait Visitor {
    fn visit<R>(&mut self)
    where
        R: RegisterValue;
}

impl<T> Visitor for &mut T
where
    T: Visitor,
{
    #[inline(always)]
    fn visit<R>(&mut self)
    where
        R: RegisterValue,
    {
        <T as Visitor>::visit::<R>(self)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct FilterVisitor<V, F> {
    pub inner: V,
    pub filter: F,
}

impl<V, F> FilterVisitor<V, F> {
    pub fn new(inner: V, filter: F) -> Self {
        Self { inner, filter }
    }
}

impl<V, F> Visitor for FilterVisitor<V, F>
where
    V: Visitor,
    F: FnMut(Register) -> bool,
{
    fn visit<R>(&mut self)
    where
        R: RegisterValue,
    {
        if (self.filter)(R::ADDRESS) {
            self.inner.visit::<R>();
        }
    }
}

pub fn visit(mut visitor: impl Visitor) {
    demod::visit(&mut visitor);
    sys::visit(&mut visitor);
    usb::visit(&mut visitor);
    // todo: others
}

/// Macro that lets us define registers more easily
macro_rules! registers {
    {
        $(
            $(#[$attrs:meta])* $name:ident: $int:ty = $block:ident $args:tt $({$($fields:tt)*})?;
        )*
    } => {
        $(
            registers!(@generate_code(($($attrs)*), $name, $int, registers!(@parse_address($block, $args)), $({$($fields)*})?));
        )*

        pub const ALL: &[Register] = &[$($name::ADDRESS),*];

        pub fn visit(mut visitor: impl Visitor) {
            $(
                visitor.visit::<$name>();
            )*
        }
    };
    (@parse_address(demod, ($page:expr, $address:expr))) => {
        Register::demod($page, $address)
    };
    (@parse_address($block:ident, ($address:expr))) => {
        Register::$block($address)
    };
    (@generate_code(($($attrs:meta)*), $name:ident, $int:ty, $address:expr, )) => {
        $(#[$attrs])*
        #[allow(non_camel_case_types)]
        #[derive(Clone, Copy, PartialEq, Eq, Debug)]
        pub struct $name(pub $int);

        registers!(@generate_impls($name, $int, $address));
    };
    (@generate_code(($($attrs:meta)*), $name:ident, $int:ty, $address:expr, {$($fields:tt)*})) => {
        bitfield! {
            $(#[$attrs])*
            #[allow(non_camel_case_types)]
            #[derive(Clone, Copy, PartialEq, Eq)]
            pub struct $name($int);
            impl Debug;
            impl new;
            $($fields)*
        }

        registers!(@generate_impls($name, $int, $address));
    };
    (@generate_impls($name:ident, $int:ty, $address:expr)) => {
        #[automatically_derived]
        impl RegisterValue for $name {
            const ADDRESS: Register = $address;
            type Bits = $int;

            fn from_bits(bits: Self::Bits) -> Self {
                Self(bits)
            }

            fn as_bits(&self) -> Self::Bits {
                self.0
            }
        }

        #[automatically_derived]
        impl From<$int> for $name {
            #[inline(always)]
            fn from(value: $int) -> Self {
                Self(value)
            }
        }

        #[automatically_derived]
        impl From<$name> for $int {
            #[inline(always)]
            fn from(value: $name) -> Self {
                value.0
            }
        }

        #[automatically_derived]
        impl Default for $name {
            #[inline(always)]
            fn default() -> Self {
                Self(Default::default())
            }
        }

    }
}

macro_rules! make_enum {
    {pub enum $name:ident($int:ty) $body:tt} => {
        make_enum!(@generate_code((pub), $name, $int, $body));
    };
    {enum $name:ident($int:ty) $body:tt} => {
        make_enum!(@generate_code((), $name, $int, $body));
    };
    (@generate_code(($($vis:tt)?), $name:ident, $int:ty, {$($variant:ident = $value:expr,)*})) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        $($vis)? enum $name {
            $($variant,)*
        }

        #[automatically_derived]
        impl From<$name> for $int {
            #[inline]
            fn from(value: $name) -> Self {
                match value {
                    $($name::$variant => $value,)*
                }
            }
        }

        #[automatically_derived]
        impl TryFrom<$int> for $name {
            type Error = $int;

            #[inline]
            fn try_from(value: $int) -> Result<Self, Self::Error> {
                match value {
                    $($value => Ok(Self::$variant),)*
                    _ => Err(value),
                }
            }
        }
    };
}

pub mod usb {
    use super::*;

    registers! {
        SYSCTL: u32 = usb(0x2000) {
            pub bool, sie_reset, set_sie_reset: 10;
            pub bool, full_packet_mode, set_full_packet_mode: 3;
            pub bool, dma_enable, set_dma_enable: 0;
        };
        EPA_CFG: u32 = usb(0x2144) {
            pub u8, isochronous_mode, set_isochronous_mode: 9, 8;
            pub bool, endpoint_enable, set_endpoint_enable: 7;
            pub u8, from try_into EndpointTransferType, endpoint_transfer_type, set_endpoint_transfer_type: 6, 5;
            pub u8, from try_into EndpointTransferDirection, endpoint_transfer_direction, set_endpoint_transfer_direction: 4, 4;
            pub u8, endpoint_number, set_endpoint_number: 3, 0;
        };
        EPA_CTL: u32 = usb(0x2148) {
            pub bool, fifo_reset, set_fifo_reset: 9;
            pub bool, fifo_flush, set_fifo_flush: 5;
            pub bool, stall_endpoint, set_stall_endpoint: 4;
            pub bool, fifo_valid, set_fifo_valid: 0;
        };

        /// Configures the max packet size.
        ///
        /// Valid values are `0..=1024` (10 bits)
        EPA_MAXPKT: u32 = usb(0x2158) {
            pub u16, max_packet_size, set_max_packet_size: 10, 0;
        };

        /// Configures FIFO
        ///
        /// `fifo_size` can be `0..=8` (3 bits)
        EPA_FIFO_CFG: u32 = usb(0x2160) {
            pub u8, block_drop_counter, set_block_drop_counter: 31, 24;
            pub u8, fifo_size, set_fifo_size: 3, 0;
        };
    }

    make_enum! {
        pub enum EndpointTransferType(u8) {
            Control = 0b00,
            Isochrronous = 0b01,
            Bulk = 0b10,
            Interrupt = 0b11,
        }
    }

    make_enum! {
        pub enum EndpointTransferDirection(u8) {
            HostToDevice = 0,
            DeviceToHost = 1,
        }
    }
}

pub mod sys {
    use super::*;

    registers! {
        /// Control register for DVB-T Demodulator
        DEMOD_CTL: u8 = sys(0x3000) {
            pub bool, pll_enable, set_pll_enable: 7;
            pub bool, adc_i_enable, set_adc_i_enable: 6;
            /// Set 0 to reset, 1 to release reset
            pub bool, hardware_reset, set_hardware_reset: 5;
            pub bool, adc_q_enable, set_adc_q_enable: 3;
        };

        /// Output Value for General-Purpose I/O
        GPO: u8 = sys(0x3001) {
            // todo
        };

        /// Input Value for General-Purpose I/O
        GPI: u8 = sys(0x3002) {
            // todo
        };

        /// Output Enable for General-Purpose I/O
        GPOE: u8 = sys(0x3003) {
            // todo
        };

        /// Direction Control for General-Purpose I/O
        GPD: u8 = sys(0x3004) {
            // todo
        };

        /// System Interrupt Enable Register
        SYSINTE: u8 = sys(0x3005) {
            // todo
        };

        /// System Interrupt Status Register
        SYSINTS: u8 = sys(0x3006) {
            // todo
        };

        /// PAD Configuration for GPIO0-GPIO3
        GP_CFG0: u8 = sys(0x3007) {
            // todo
        };

        /// PAD Configuration for GPIO0-GPIO3
        GP_CFG4: u8 = sys(0x3008) {
            // todo
        };

        /// System Interrupt Enable Register (GPIO5-GPIO7)
        SYSINTE_1: u8 = sys(0x3009) {
            // todo
        };

        /// System Interrupt Status Register (GPIO5-GPIO7)
        SYSINTS_1: u8 = sys(0x300a) {
            // todo
        };

        /// Enable IR Remote Wakeup & Low Current XTL Mode when suspended
        ///
        /// This is only listed in the register map, but no further description is given. `librtlsdr` sets this to `0x22`. After powerup this is `0x02`.
        ///
        DEMOD_CTL_1: u8 = sys(0x300b) {
            // todo
        };

        /// IR Sensor Discontinuous Turned ON. Contrrolled by GPIO3
        IR_SUSPEND: u8 = sys(0x300c) {
            // todo
        };

        // todo: IrDA registers

        // todo: I2C master registers
    }
}

pub mod demod {
    pub use super::*;
    use crate::rtl2832u::FirFilter;

    // this seems to be a better list of registers:
    // https://github.com/jaredquinn/DVB-Realtek-RTL2832U/blob/3c9e21225d2292fe0e6b885cd861fbebb890918a/src/demod_rtl2832.c#L1631
    //
    // the following register map was generated from the linux source

    registers! {
        // todo: maybe merge this with the next since they both contain mpeg_io_opt field?
        OPT_ADC_IQ_MPEG_IO_OPT_2_2: u8 = demod(0, 0x06) {
            /// OPT_ADC_IQ: 0, 0x06
            ///
            /// Exchange ADC_I and ADC_Q datapath
            ///
            /// Why is this 2 bits??? The datasheet only says 0=default, 1=swapped
            pub u8, opt_adc_iq, set_opt_adc_iq: 5, 4;
            /// MPEG_IO_OPT_2_2: 0, 0x06
            pub bool, mpeg_io_opt_2_2, set_mpeg_io_opt_2_2: 7;
        };
        MPEG_IO_OPT_1_0: u8 = demod(0, 0x07) {
            /// MPEG_IO_OPT_1_0: 0, 0x07
            pub u8, mpeg_io_opt_1_0, set_mpeg_io_opt_1_0: 7, 6;
        };
        AD_EN_REG1_AD_EN_REG: u8 = demod(0, 0x08) {
            /// AD_EN_REG1: 0, 0x08
            ///
            /// Enable ADC_Q
            pub bool, ad_en_reg1, set_ad_en_reg1: 6;
            /// AD_EN_REG: 0, 0x08
            ///
            /// Enable ADC_I
            pub bool, ad_en_reg, set_ad_en_reg: 7;
        };
        AD_AVI_AD_AVQ_AD_AV_REF: u8 = demod(0, 0x09) {
            /// AD_AVI: 0, 0x09
            pub u8, ad_avi, set_ad_avi: 1, 0;
            /// AD_AVQ: 0, 0x09
            pub u8, ad_avq, set_ad_avq: 3, 2;
            /// AD_AV_REF: 0, 0x09
            pub u8, ad_av_ref, set_ad_av_ref: 6, 0;
        };
        REG_PI: u8 = demod(0, 0x0a) {
            /// REG_PI: 0, 0x0a
            pub u8, reg_pi, set_reg_pi: 2, 0;
        };
        REG_MON_REG_MONSEL_REG_GPE: u8 = demod(0, 0x0d) {
            /// REG_MON: 0, 0x0d
            ///
            /// 0b11 on powerup
            pub u8, reg_mon, set_reg_mon: 1, 0;

            /// REG_MONSEL: 0, 0x0d
            pub bool, reg_monsel, set_reg_monsel: 2;
            /// REG_GPE: 0, 0x0d
            pub bool, reg_gpe, set_reg_gpe: 7;
        };
        POLAR_IF_AGC_POLAR_RF_AGC: u8 = demod(0, 0x0e) {
            /// POLAR_IF_AGC: 0, 0x0e
            pub bool, polar_if_agc, set_polar_if_agc: 0;
            /// POLAR_RF_AGC: 0, 0x0e
            pub bool, polar_rf_agc, set_polar_rf_agc: 1;
        };
        REG_GPO: u8 = demod(0, 0x10) {
            /// REG_GPO: 0, 0x10
            pub bool, reg_gpo, set_reg_gpo: 0;
        };
        AD7_SETTING: u16 = demod(0, 0x11) {
            /// AD7_SETTING: 0, 0x11
            pub u16, ad7_setting, set_ad7_setting: 15, 0;
        };
        REG_4MSEL: u8 = demod(0, 0x13) {
            /// REG_4MSEL: 0, 0x13
            pub bool, reg_4msel, set_reg_4msel: 0;
        };
        /// Another undocumented register
        ///
        /// Both linux sdr and librtlsdr set this to 0x05 during baseband initialization.
        ///
        /// librtlsdr comment: "enable SDR mode, disable DAGC (bit 5)"
        ///
        /// - `rtlsdr_set_testmode` sets this to 0x03 (if on=true) or 0x05 (otherwise)
        /// - `rtlsdr_set_agc_mode` sets this to 0x25 (if on=true) or 0x05 (otherwise)
        /// - linux sdr sets this to 0x20 in `rtl2832_sdr_unset_adc`, called from `rtl2832_sdr_stop_streaming`
        ///
        /// The datasheet lists `en_dagc` in page 0 offset 0x11 bit 0
        ///
        /// In dumps:
        ///
        /// - 0x20 at startup
        /// - 0x03 in rtl_test
        ///
        /// So:
        ///
        /// - bit 5: dagc
        /// - bit 2: on if not in test mode (send samples?)
        /// - bit 1: on if in test mode (send counter?)
        /// - bit 0: (enable streaming?)
        ///
        UNK_DAGC: u8 = demod(0, 0x19) {
            pub bool, enable_dagc, set_enable_dagc: 5;
            pub bool, unk_2, set_unk_2: 2;
            pub bool, test_mode, set_test_mode: 1;
            pub bool, unk_0, set_unk_0: 0;

        };
        PIP_ON: u8 = demod(0, 0x21) {
            /// PIP_ON: 0, 0x21
            pub bool, pip_on, set_pip_on: 3;
        };
        /// librtlsdr disables all but bits 5 and 6 on baseband init
        PID_CTL: u8 = demod(0, 0x61) {
            /// Pass all error packets
            pub bool, err_pass, set_err_pass: 5;
            /// Reject matched packets
            pub bool, mode, set_mode: 6;
            /// Enable PID output
            ///
            /// TODO: We think disabling this will reject all packet. Test this with the mode setting.
            pub bool, enable, set_enable: 7;
        };
        PID_ENABLE: u32 = demod(0, 0x62);
        // todo: this or individual registers? also [u16; 32] doesn't implement our trait yet.
        //PID_VALUE: [u16; 32] = demod(0, 0x66);
        VAL_LVL_PIP_ERR_LVL_PIP_SYNC_LVL_PIP_CKOUT_PWR_PIP_CKOUTPAR_PIP: u8 = demod(0, 0xb7) {
            /// VAL_LVL_PIP: 0, 0xb7
            pub bool, val_lvl_pip, set_val_lvl_pip: 0;
            /// ERR_LVL_PIP: 0, 0xb7
            pub bool, err_lvl_pip, set_err_lvl_pip: 1;
            /// SYNC_LVL_PIP: 0, 0xb7
            pub bool, sync_lvl_pip, set_sync_lvl_pip: 2;
            /// CKOUT_PWR_PIP: 0, 0xb7
            pub bool, ckout_pwr_pip, set_ckout_pwr_pip: 3;
            /// CKOUTPAR_PIP: 0, 0xb7
            pub bool, ckoutpar_pip, set_ckoutpar_pip: 4;
        };
        VAL_LVL_PID_ERR_LVL_PID_SYNC_LVL_PID_CKOUT_PWR_PID_CKOUTPAR_PID: u8 = demod(0, 0xb9) {
            /// VAL_LVL_PID: 0, 0xb9
            pub bool, val_lvl_pid, set_val_lvl_pid: 0;
            /// ERR_LVL_PID: 0, 0xb9
            pub bool, err_lvl_pid, set_err_lvl_pid: 1;
            /// SYNC_LVL_PID: 0, 0xb9
            pub bool, sync_lvl_pid, set_sync_lvl_pid: 2;
            /// CKOUT_PWR_PID: 0, 0xb9
            pub bool, ckout_pwr_pid, set_ckout_pwr_pid: 3;
            /// CKOUTPAR_PID: 0, 0xb9
            pub bool, ckoutpar_pid, set_ckoutpar_pid: 4;
        };
        /// Only the I2C repeater enable bit is documented.
        ///
        /// `librtlsdr` sets this to `0x14` and `0x10` in `rtlsdr_init_baseband`
        /// with comment `reset demod (bit 3, soft_rst)`. It's really bit 2 if you
        /// count properly.
        SOFT_RST_IIC_REPEAT: u8 = demod(1, 0x01) {
            /// SOFT_RST: 1, 0x01
            pub bool, soft_rst, set_soft_rst: 2;
            /// IIC_REPEAT: 1, 0x01
            pub bool, iic_repeat, set_iic_repeat: 3;
        };
        AGC_TARG_VAL_0: u8 = demod(1, 0x02) {
            /// AGC_TARG_VAL_0: 1, 0x02
            pub bool, agc_targ_val_0, set_agc_targ_val_0: 0;
        };
        AGC_TARG_VAL_8_1: u8 = demod(1, 0x03) {
            /// AGC_TARG_VAL_8_1: 1, 0x03
            pub u8, agc_targ_val_8_1, set_agc_targ_val_8_1: 7, 0;
        };
        LOOP_GAIN2_3_0_AAGC_HOLD_EN_RF_AGC_EN_IF_AGC: u8 = demod(1, 0x04) {
            /// LOOP_GAIN2_3_0: 1, 0x04
            pub u8, loop_gain2_3_0, set_loop_gain2_3_0: 4, 1;
            /// AAGC_HOLD: 1, 0x04
            pub bool, aagc_hold, set_aagc_hold: 5;
            /// EN_RF_AGC: 1, 0x04
            pub bool, en_rf_agc, set_en_rf_agc: 6;
            /// EN_IF_AGC: 1, 0x04
            pub bool, en_if_agc, set_en_if_agc: 7;
        };
        LOOP_GAIN2_4: u8 = demod(1, 0x05) {
            /// LOOP_GAIN2_4: 1, 0x05
            pub bool, loop_gain2_4, set_loop_gain2_4: 7;
        };
        VTOP1: u8 = demod(1, 0x06) {
            /// VTOP1: 1, 0x06
            pub u8, vtop1, set_vtop1: 5, 0;
        };
        KRF2: u8 = demod(1, 0x07) {
            /// KRF2: 1, 0x07
            pub u8, krf2, set_krf2: 7, 0;
        };
        IF_AGC_MIN: u8 = demod(1, 0x08) {
            /// IF_AGC_MIN: 1, 0x08
            pub u8, if_agc_min, set_if_agc_min: 7, 0;
        };
        IF_AGC_MAX: u8 = demod(1, 0x09) {
            /// IF_AGC_MAX: 1, 0x09
            pub u8, if_agc_max, set_if_agc_max: 7, 0;
        };
        EN_DAGC: u8 = demod(1, 0x11) {
            pub bool, en_dagc, set_endagc: 0;
        };
        RF_AGC_MIN: u8 = demod(1, 0x0a) {
            /// RF_AGC_MIN: 1, 0x0a
            pub u8, rf_agc_min, set_rf_agc_min: 7, 0;
        };
        RF_AGC_MAX: u8 = demod(1, 0x0b) {
            /// RF_AGC_MAX: 1, 0x0b
            pub u8, rf_agc_max, set_rf_agc_max: 7, 0;
        };
        IF_AGC_MAN_IF_AGC_MAN_VAL: u16 = demod(1, 0x0c) {
            /// IF_AGC_MAN: 1, 0x0c
            pub bool, if_agc_man, set_if_agc_man: 6;
            /// IF_AGC_MAN_VAL: 1, 0x0c
            pub u16, if_agc_man_val, set_if_agc_man_val: 13, 0;
        };
        RF_AGC_MAN_RF_AGC_MAN_VAL: u16 = demod(1, 0x0e) {
            /// RF_AGC_MAN: 1, 0x0e
            pub bool, rf_agc_man, set_rf_agc_man: 6;
            /// RF_AGC_MAN_VAL: 1, 0x0e
            pub u16, rf_agc_man_val, set_rf_agc_man_val: 13, 0;
        };
        DAGC_TRG_VAL: u8 = demod(1, 0x12) {
            /// DAGC_TRG_VAL: 1, 0x12
            pub u8, dagc_trg_val, set_dagc_trg_val: 7, 0;
        };
        SPEC_INV: u8 = demod(1, 0x15) {
            /// SPEC_INV: 1, 0x15
            pub bool, spec_inv, set_spec_inv: 0;
            /// En_aci
            pub bool, en_aci, set_en_aci: 1;
        };

        /// See PSET_IFFREQ
        UNK_DDC_OFFSET: u16 = demod(1, 0x16) {};

        /// This really starts at 0x19, but we expand it to 32 bit
        ///
        /// There's supposedly a DDC offset somewhere between 0x16 and PSET_IFFREQ
        ///
        /// This is how it looks like after powerup:
        ///
        /// ```plain
        /// │00000010│ 0a 07 39 10 04 00 3f ce ┊ cc 35 d7 5d 09 f6 d2 a7 │_•9••⋄?×┊×5×]_×××│
        /// ```
        ///
        /// [Linux sdr driver][1] also sets this to 0
        ///
        /// [1]: https://code.googlesource.com/linux/torvalds/linux/+/6d36c728bc2e2d632f4b0dea00df5532e20dfdab/drivers/media/dvb-frontends/rtl2832_sdr.c#509
        PSET_IFFREQ: u32 = demod(1, 0x18) {
            /// PSET_IFFREQ: 1, 0x19
            pub u32, pset_iffreq, set_pset_iffreq: 21, 0;
        };
        /// Not in datasheet, but both librtlsdr and the linux sdr driver put a 20 byte FIR filter here.
        ///
        /// See [`FirFilter`](super::FirFilter)
        UNK_FIR_FILTER: [u8; 20] = demod(1, 0x1c);
        EN_CACQ_NOTCH: u8 = demod(1, 0x61) {
            /// EN_CACQ_NOTCH: 1, 0x61
            pub bool, en_cacq_notch, set_en_cacq_notch: 4;
        };
        KB_P1_KB_P2: u8 = demod(1, 0x64) {
            /// KB_P1: 1, 0x64
            pub u8, kb_p1, set_kb_p1: 3, 1;
            /// KB_P2: 1, 0x64
            pub u8, kb_p2, set_kb_p2: 6, 4;
        };
        KB_P3: u8 = demod(1, 0x65) {
            /// KB_P3: 1, 0x65
            pub u8, kb_p3, set_kb_p3: 2, 0;
        };
        EST_KQ: u16 = demod(1, 0x66) {
            /// Est_kq
            ///
            /// Estimated Gain for IQ Gain Mismatch, u(12, 11f)
            ///
            /// read-only
            pub u16, est_kq, set_est_kq: 11, 0;
        };
        EST_SIN: u16 = demod(1, 0x68) {
            /// Est_sin
            ///
            /// Estimated Sin for IQ `\theta` Mismatch, u(12, 10f)
            ///
            /// read-only
            pub u16, est_kq, set_est_kq: 11, 0;
        };
        TRK_KS_P2: u8 = demod(1, 0x6f) {
            /// TRK_KS_P2: 1, 0x6f
            pub u8, trk_ks_p2, set_trk_ks_p2: 2, 0;
        };
        TRK_KS_I2: u8 = demod(1, 0x70) {
            /// TRK_KS_I2: 1, 0x70
            pub u8, trk_ks_i2, set_trk_ks_i2: 5, 3;
        };
        TR_THD_SET2: u8 = demod(1, 0x72) {
            /// TR_THD_SET2: 1, 0x72
            pub u8, tr_thd_set2, set_tr_thd_set2: 3, 0;
        };
        TRK_KC_P2: u8 = demod(1, 0x73) {
            /// TRK_KC_P2: 1, 0x73
            pub u8, trk_kc_p2, set_trk_kc_p2: 5, 3;
        };
        TRK_KC_I2: u8 = demod(1, 0x75) {
            /// TRK_KC_I2: 1, 0x75
            pub u8, trk_kc_i2, set_trk_kc_i2: 2, 0;
        };
        CR_THD_SET2: u8 = demod(1, 0x76) {
            /// CR_THD_SET2: 1, 0x76
            pub u8, cr_thd_set2, set_cr_thd_set2: 7, 6;
        };
        CKOUTPAR_CKOUT_PWR_SYNC_DUR: u8 = demod(1, 0x7b) {
            /// CKOUTPAR: 1, 0x7b
            pub bool, ckoutpar, set_ckoutpar: 5;
            /// CKOUT_PWR: 1, 0x7b
            pub bool, ckout_pwr, set_ckout_pwr: 6;
            /// SYNC_DUR: 1, 0x7b
            pub bool, sync_dur, set_sync_dur: 7;
        };
        ERR_DUR_SYNC_LVL_ERR_LVL_VAL_LVL_SERIAL_SER_LSB: u8 = demod(1, 0x7c) {
            /// ERR_DUR: 1, 0x7c
            pub bool, err_dur, set_err_dur: 0;
            /// SYNC_LVL: 1, 0x7c
            pub bool, sync_lvl, set_sync_lvl: 1;
            /// ERR_LVL: 1, 0x7c
            pub bool, err_lvl, set_err_lvl: 2;
            /// VAL_LVL: 1, 0x7c
            pub bool, val_lvl, set_val_lvl: 3;
            /// SERIAL: 1, 0x7c
            pub bool, serial, set_serial: 4;
            /// SER_LSB: 1, 0x7c
            pub bool, ser_lsb, set_ser_lsb: 5;
        };
        CDIV_PH0_CDIV_PH1: u8 = demod(1, 0x7d) {
            /// CDIV_PH0: 1, 0x7d
            pub u8, cdiv_ph0, set_cdiv_ph0: 3, 0;
            /// CDIV_PH1: 1, 0x7d
            pub u8, cdiv_ph1, set_cdiv_ph1: 7, 4;
        };
        TR_WAIT_MIN_8K: u16 = demod(1, 0x88) {
            /// TR_WAIT_MIN_8K: 1, 0x88
            pub u16, tr_wait_min_8k, set_tr_wait_min_8k: 11, 2;
        };
        RSD_BER_FAIL_VAL: u16 = demod(1, 0x8f) {
            /// RSD_BER_FAIL_VAL: 1, 0x8f
            pub u16, rsd_ber_fail_val, set_rsd_ber_fail_val: 15, 0;
        };
        /// # FSM state
        ///
        /// Linux SDR just comments "FSM" and sets this to 0x0ff0.
        /// It actually writes `\x00\xf0\x0f` to offset 0x92 in `rtl2832_sdr_set_adc`.
        /// In `rtl2832_sdr_unset_adc` it resets it to `\x00\x0f\xff` (0xff0f)
        ///
        /// librtlsdr comments "init FSM state-holding register" and writes 0xff0 (doesn't write to 0x92)
        ///
        /// Dumps:
        /// - rtl_test: 0x0ff0
        /// - powerup: 0xff0f
        ///
        /// There's also FSM_STAGE: 3, 0x51, bits 6:3
        ///
        /// This is in the dvbt driver
        ///
        /// ```no_run
        /// SM_PASS: u16 = demod(1, 0x93) {
        ///     /// SM_PASS: 1, 0x93
        ///     pub u16, sm_pass, set_sm_pass: 11, 0;
        /// };
        /// ```
        UNK_FSM: u16 = demod(1, 0x93);

        MGD_THD0: u8 = demod(1, 0x95) {
            /// MGD_THD0: 1, 0x95
            pub u8, mgd_thd0, set_mgd_thd0: 7, 0;
        };
        MGD_THD1: u8 = demod(1, 0x96) {
            /// MGD_THD1: 1, 0x96
            pub u8, mgd_thd1, set_mgd_thd1: 7, 0;
        };
        MGD_THD2: u8 = demod(1, 0x97) {
            /// MGD_THD2: 1, 0x97
            pub u8, mgd_thd2, set_mgd_thd2: 7, 0;
        };
        MGD_THD3: u8 = demod(1, 0x98) {
            /// MGD_THD3: 1, 0x98
            pub u8, mgd_thd3, set_mgd_thd3: 7, 0;
        };
        MGD_THD4: u8 = demod(1, 0x99) {
            /// MGD_THD4: 1, 0x99
            pub u8, mgd_thd4, set_mgd_thd4: 7, 0;
        };
        MGD_THD5: u8 = demod(1, 0x9a) {
            /// MGD_THD5: 1, 0x9a
            pub u8, mgd_thd5, set_mgd_thd5: 7, 0;
        };
        MGD_THD6: u8 = demod(1, 0x9b) {
            /// MGD_THD6: 1, 0x9b
            pub u8, mgd_thd6, set_mgd_thd6: 7, 0;
        };
        MGD_THD7: u8 = demod(1, 0x9c) {
            /// MGD_THD7: 1, 0x9c
            pub u8, mgd_thd7, set_mgd_thd7: 7, 0;
        };
        /// These two overlap in a very confusing way. Can't be combined into 32bit either.
        ///
        /// ```plain
        ///        76543210
        /// 0x9b   --------
        /// 0x9c   --------
        /// 0x9d   cccccccc
        /// 0x9e   cccccccc
        /// 0x9f   ccccrrrr
        /// 0xa0   rrrrrrrr
        /// 0xa1   rrrrrrrr
        /// 0xa2   rrrrrr--
        /// ```
        CFREQ_OFF_RATIO_RSAMP_RATIO: u64 = demod(1, 0x9b) {
            /// CFREQ_OFF_RATIO: 1, 0x9d
            pub u32, cfreq_off_ratio, set_cfreq_off_ratio: 47, 28;
            /// RSAMP_RATIO: 1, 0x9f
            pub u32, rsamp_ratio, set_rsamp_ratio: 27, 2;
        };
        EN_BK_TRK: u8 = demod(1, 0xa6) {
            /// EN_BK_TRK: 1, 0xa6
            pub bool, en_bk_trk, set_en_bk_trk: 7;
        };
        DC_CANCEL: u8 = demod(1, 0xb1) {
            /// EN_BBIN: 1, 0xb1
            ///
            /// Enable Zero-IF input
            pub bool, en_bbin, set_en_bbin: 0;

            /// en_dc_est
            ///
            /// Enable DC estimation and cancellation
            pub bool, en_dc_est, set_en_dc_est: 1;

            /// unknown bit.
            ///
            /// 1 on powerup
            pub bool, unk_2, set_unk_2: 2;

            /// en_iq_comp
            ///
            /// Enable IQ compensation
            pub bool, en_iq_comp, set_en_iq_comp: 3;

            /// en_iq_est
            ///
            /// Enable IQ estimation for compensation
            pub bool, en_iq_est, set_en_iq_est: 4;
        };
        AAGC_LOOP_GAIN: u8 = demod(1, 0xc7) {
            /// AAGC_LOOP_GAIN: 1, 0xc7
            pub u8, aagc_loop_gain, set_aagc_loop_gain: 5, 1;
        };
        LOOP_GAIN3: u8 = demod(1, 0xc8) {
            /// LOOP_GAIN3: 1, 0xc8
            pub u8, loop_gain3, set_loop_gain3: 4, 0;
        };
        VTOP2: u8 = demod(1, 0xc9) {
            /// VTOP2: 1, 0xc9
            pub u8, vtop2, set_vtop2: 5, 0;
        };
        VTOP3: u8 = demod(1, 0xca) {
            /// VTOP3: 1, 0xca
            pub u8, vtop3, set_vtop3: 5, 0;
        };
        KRF1: u8 = demod(1, 0xcb) {
            /// KRF1: 1, 0xcb
            pub u8, krf1, set_krf1: 7, 0;
        };
        KRF3: u8 = demod(1, 0xcd) {
            /// KRF3: 1, 0xcd
            pub u8, krf3, set_krf3: 7, 0;
        };
        KRF4: u8 = demod(1, 0xce) {
            /// KRF4: 1, 0xce
            pub u8, krf4, set_krf4: 7, 0;
        };
        EN_AGC_PGA: u8 = demod(1, 0xd7) {
            /// EN_AGC_PGA: 1, 0xd7
            pub bool, en_agc_pga, set_en_agc_pga: 0;
        };
        INTER_CNT_LEN: u8 = demod(1, 0xd8) {
            /// INTER_CNT_LEN: 1, 0xd8
            pub u8, inter_cnt_len, set_inter_cnt_len: 3, 0;
        };
        THD_LOCK_UP: u8 = demod(1, 0xd9) {
            /// THD_LOCK_UP: 1, 0xd9
            pub u16, thd_lock_up, set_thd_lock_up: 8, 0;
        };
        THD_LOCK_DW: u8 = demod(1, 0xdb) {
            /// THD_LOCK_DW: 1, 0xdb
            pub u16, thd_lock_dw, set_thd_lock_dw: 8, 0;
        };
        THD_UP1: u8 = demod(1, 0xdd) {
            /// THD_UP1: 1, 0xdd
            pub u8, thd_up1, set_thd_up1: 7, 0;
        };
        THD_DW1: u8 = demod(1, 0xde) {
            /// THD_DW1: 1, 0xde
            pub u8, thd_dw1, set_thd_dw1: 7, 0;
        };
        EN_GI_PGA: u8 = demod(1, 0xe5) {
            /// EN_GI_PGA: 1, 0xe5
            pub bool, en_gi_pga, set_en_gi_pga: 0;
        };
        GI_PGA_STATE: u8 = demod(1, 0xe6) {
            /// GI_PGA_STATE: 1, 0xe6
            pub bool, gi_pga_state, set_gi_pga_state: 3;
        };
        SCALE1_B92: u8 = demod(2, 0x92) {
            /// SCALE1_B92: 2, 0x92
            pub u8, scale1_b92, set_scale1_b92: 7, 0;
        };
        SCALE1_B93: u8 = demod(2, 0x93) {
            /// SCALE1_B93: 2, 0x93
            pub u8, scale1_b93, set_scale1_b93: 7, 0;
        };
        SCALE1_BA7: u8 = demod(2, 0xa7) {
            /// SCALE1_BA7: 2, 0xa7
            pub u8, scale1_ba7, set_scale1_ba7: 7, 0;
        };
        SCALE1_BA9: u8 = demod(2, 0xa9) {
            /// SCALE1_BA9: 2, 0xa9
            pub u8, scale1_ba9, set_scale1_ba9: 7, 0;
        };
        SCALE1_BAA: u8 = demod(2, 0xaa) {
            /// SCALE1_BAA: 2, 0xaa
            pub u8, scale1_baa, set_scale1_baa: 7, 0;
        };
        SCALE1_BAB: u8 = demod(2, 0xab) {
            /// SCALE1_BAB: 2, 0xab
            pub u8, scale1_bab, set_scale1_bab: 7, 0;
        };
        SCALE1_BAC: u8 = demod(2, 0xac) {
            /// SCALE1_BAC: 2, 0xac
            pub u8, scale1_bac, set_scale1_bac: 7, 0;
        };
        K1_CR_STEP12: u16 = demod(2, 0xad) {
            /// K1_CR_STEP12: 2, 0xad
            pub u8, k1_cr_step12, set_k1_cr_step12: 9, 4;
        };
        SCALE1_BB0: u8 = demod(2, 0xb0) {
            /// SCALE1_BB0: 2, 0xb0
            pub u8, scale1_bb0, set_scale1_bb0: 7, 0;
        };
        SCALE1_BB1: u8 = demod(2, 0xb1) {
            /// SCALE1_BB1: 2, 0xb1
            pub u8, scale1_bb1, set_scale1_bb1: 7, 0;
        };
        RSSI_R: u8 = demod(3, 0x01) {
            /// RSSI_R: 3, 0x01
            pub u8, rssi_r, set_rssi_r: 6, 0;
        };
        DAGC_VAL: u8 = demod(3, 0x05) {
            /// DAGC_VAL: 3, 0x05
            pub u8, dagc_val, set_dagc_val: 7, 0;
        };
        ACI_DET_IND: u8 = demod(3, 0x12) {
            /// ACI_DET_IND: 3, 0x12
            pub bool, aci_det_ind, set_aci_det_ind: 0;
        };
        SFREQ_OFF: u16 = demod(3, 0x18) {
            /// SFREQ_OFF: 3, 0x18
            pub u16, sfreq_off, set_sfreq_off: 13, 0;
        };
        RX_CONSTEL_RX_HIER: u8 = demod(3, 0x3c) {
            /// RX_CONSTEL: 3, 0x3c
            pub u8, rx_constel, set_rx_constel: 3, 2;
            /// RX_HIER: 3, 0x3c
            pub u8, rx_hier, set_rx_hier: 6, 4;
        };
        RX_C_RATE_LP_RX_C_RATE_HP: u8 = demod(3, 0x3d) {
            /// RX_C_RATE_LP: 3, 0x3d
            pub u8, rx_c_rate_lp, set_rx_c_rate_lp: 2, 0;
            /// RX_C_RATE_HP: 3, 0x3d
            pub u8, rx_c_rate_hp, set_rx_c_rate_hp: 5, 3;
        };
        RSD_BER_EST: u16 = demod(3, 0x4e) {
            /// RSD_BER_EST: 3, 0x4e
            pub u16, rsd_ber_est, set_rsd_ber_est: 15, 0;
        };
        GI_IDX_FFT_MODE_IDX_FSM_STAGE: u8 = demod(3, 0x51) {
            /// GI_IDX: 3, 0x51
            pub u8, gi_idx, set_gi_idx: 1, 0;
            /// FFT_MODE_IDX: 3, 0x51
            pub bool, fft_mode_idx, set_fft_mode_idx: 2;
            /// FSM_STAGE: 3, 0x51
            pub u8, fsm_stage, set_fsm_stage: 6, 3;
        };
        IF_AGC_VAL: u16 = demod(3, 0x59) {
            /// IF_AGC_VAL: 3, 0x59
            pub u16, if_agc_val, set_if_agc_val: 13, 0;
        };
        RF_AGC_VAL: u16 = demod(3, 0x5b) {
            /// RF_AGC_VAL: 3, 0x5b
            pub u16, rf_agc_val, set_rf_agc_val: 13, 0;
        };
        CFREQ_OFF: u32 = demod(3, 0x5e) {
            /// CFREQ_OFF: 3, 0x5f
            pub u32, cfreq_off, set_cfreq_off: 17, 0;
        };
        CE_EST_EVM: u16 = demod(4, 0x0c) {
            /// CE_EST_EVM: 4, 0x0c
            pub u16, ce_est_evm, set_ce_est_evm: 15, 0;
        };
    }

    impl UNK_FIR_FILTER {
        #[inline(always)]
        pub fn decode(&self) -> FirFilter {
            FirFilter::decode(&self.0)
        }

        #[inline(always)]
        pub fn encode(&mut self, filter: &FirFilter) {
            filter.encode(&mut self.0);
        }

        #[inline(always)]
        pub fn from_filter(filter: &FirFilter) -> Self {
            let mut buffer = Self::default();
            buffer.encode(&filter);
            buffer
        }
    }

    impl PID_ENABLE {
        pub fn en_pid(&self, pid: u8) -> bool {
            assert!(pid < 32, "PID out of range: {pid}");
            self.0 & (1 << pid) != 0
        }

        pub fn set_en_pid(&mut self, pid: u8) {
            assert!(pid < 32, "PID out of range: {pid}");
            self.0 |= 1 << pid;
        }

        pub fn clear(&mut self) {
            self.0 = 0;
        }

        pub fn set_all(&mut self) {
            self.0 = !0;
        }
    }
}
