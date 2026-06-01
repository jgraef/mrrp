//! Low-level interface for the RTL2832U
//!
//! - [Overview][3]
//! - [Datasheet][1]
//! - [`librtlsdr` (blog)][2],
//! - [linux 'rtl2832_sdr.c``][4]
//!
//! # Note
//!
//! When porting from `librtlsdr` it can be very confusing, because it looks
//! like they have their byteorder mixed up. They're writing registers a
//! big-endian, while the RTL232U uses little endian.
//!
//! [1]: https://homepages.uni-regensburg.de/~erc24492/SDR/Data_rtl2832u.pdf
//! [2]: https://github.com/rtlsdrblog/rtl-sdr-blog/blob/master/src/librtlsdr.c
//! [3]: https://homepages.uni-regensburg.de/~erc24492/SDR/RTL2832U.pdf
//! [4]: https://code.googlesource.com/linux/torvalds/linux/+/6d36c728bc2e2d632f4b0dea00df5532e20dfdab/drivers/media/dvb-frontends/rtl2832_sdr.c

pub mod register;

use std::{
    fmt::Debug,
    ops::Deref,
    time::Duration,
};

use crate::{
    Error,
    i2c::I2cRepeater,
    rtl2832u::register::{
        self as reg,
        Bits,
        Register,
        RegisterValue,
    },
};

#[derive(Debug)]
pub struct Rtl2832u {
    usb_interface: nusb::Interface,
    control_timeout: Duration,
}

impl Rtl2832u {
    pub(crate) fn new(usb_interface: nusb::Interface, control_timeout: Duration) -> Self {
        Self {
            usb_interface,
            control_timeout,
        }
    }

    pub fn i2c_repeater(&mut self) -> I2cRepeater<'_> {
        // notes:
        //
        // it seems the i2c_repeater enable bit only needs to be set for accessing the
        // tuner. it's usually disconnected.
        //
        // but e.g. the eeprom can be via I2C block with the eeproms I2C address (0xa0?)
        // as address, and then:
        //  - write a 1 byte address for reading
        //  - write a 2 bytes, address and value for writing
        // (see read/write eeprom librtlsdr code)
        //
        // also grepping for rtlsdr_set_i2c_repeater shows that it's only enabled for
        // tuner access.
        //
        I2cRepeater::new(&mut self.usb_interface)
    }

    pub async fn read(&mut self, address: Register, length: u16) -> Result<Vec<u8>, Error> {
        let request = address.control_in(length);

        tracing::debug!(?request, "sending control request");

        // wish they didn't allocate
        let response_data = self
            .usb_interface
            .control_in(request, self.control_timeout)
            .await?;

        if response_data.len() != response_data.len() {
            return Err(Error::InvalidControlResponse {
                expected_length: length,
                response_length: response_data.len(),
            });
        }

        Ok(response_data)
    }

    pub async fn write(&mut self, address: Register, data: &[u8]) -> Result<(), Error> {
        let request = address.control_out(data);

        tracing::debug!(?request, "sending control request");

        self.usb_interface
            .control_out(request, self.control_timeout)
            .await?;
        Ok(())
    }

    pub async fn read_register<R>(&mut self) -> Result<R, Error>
    where
        R: RegisterValue + Debug,
    {
        let data = self
            .read(R::ADDRESS, <R::Bits as register::Bits>::LENGTH)
            .await?;

        let bits = <R::Bits as register::Bits>::from_bytes(&data);
        let value = R::from_bits(bits);

        tracing::debug!(address = ?R::ADDRESS, ?value, "read register");

        Ok(value)
    }

    pub async fn write_register<R>(&mut self, value: R) -> Result<(), Error>
    where
        R: RegisterValue + Debug,
    {
        tracing::debug!(address = ?R::ADDRESS, ?value, "writing register");

        let bits = value.as_bits();
        let data = bits.into_bytes();

        self.write(R::ADDRESS, data.as_ref()).await
    }

    pub async fn write_register_with<R>(&mut self, f: impl FnOnce(&mut R)) -> Result<(), Error>
    where
        R: RegisterValue + Default + Debug,
        <R as RegisterValue>::Bits: Debug,
    {
        let mut value = Default::default();
        f(&mut value);

        self.write_register(value).await
    }

    pub async fn reset(&mut self) -> Result<(), Error> {
        tracing::debug!("resetting device");

        self.write_register_with::<reg::sys::DEMOD_CTL>(|demod_ctl| {
            demod_ctl.set_hardware_reset(false); // 1=reset
        })
        .await?;

        tokio::time::sleep(Duration::from_millis(5)).await;

        self.write_register_with::<reg::sys::DEMOD_CTL>(|demod_ctl| {
            demod_ctl.set_hardware_reset(true); // 1=release
        })
        .await?;

        Ok(())
    }

    pub async fn initialize_baseband(&mut self) -> Result<(), Error> {
        // check librtlsdr, but also [linux driver][1]
        //
        // [1]: https://github.com/jaredquinn/DVB-Realtek-RTL2832U/blob/3c9e21225d2292fe0e6b885cd861fbebb890918a/src/rtl2832u_fe.c#L658

        // initialize USB

        // enable DMA and enable full packet mode
        self.write_register_with::<reg::usb::SYSCTL>(|sysctl| {
            sysctl.set_dma_enable(true);
            sysctl.set_full_packet_mode(true);
        })
        .await?;

        // set max packet size to 512
        self.write_register_with::<reg::usb::EPA_MAXPKT>(|epa_maxpkt| {
            epa_maxpkt.set_max_packet_size(512);
        })
        .await?;

        // stall endpoint, fifo reset
        self.write_register_with::<reg::usb::EPA_CTL>(|epa_ctl| {
            epa_ctl.set_stall_endpoint(true);
            epa_ctl.set_fifo_reset(true);
        })
        .await?;

        // poweron demod

        // I don't know what this does (see comment on DEMOD_CTL_1). It's 0x02 on
        // powerup, and librtlsdr writes 0x22. I don't see why they would enable IrDA
        // remote wakeup, so maybe it enables low current XTL mode?
        //
        // the linux driver clears bits 2 and 3 at startup, but doesn't use it
        // otherwise.
        //
        //self.write_register(reg::sys::DEMOD_CTL_1(0x22)).await?;

        // demod PLL enable, release reset, ADC_I enable, ADC_Q enable
        //
        // note: the PLL needs to be on for the demod registers to work
        self.write_register_with::<reg::sys::DEMOD_CTL>(|demod_ctl| {
            demod_ctl.set_pll_enable(true);
            demod_ctl.set_adc_i_enable(true);
            demod_ctl.set_hardware_reset(true); // 1=release
            demod_ctl.set_adc_q_enable(true);
        })
        .await?;

        // reset demod
        let mut iic_repeat = reg::demod::SOFT_RST_IIC_REPEAT(0x10);
        iic_repeat.set_soft_rst(true);
        self.write_register(iic_repeat).await?;
        iic_repeat.set_soft_rst(false);
        self.write_register(iic_repeat).await?;

        // disable spectrum inversion and adjacent channel rejection
        self.write_register_with::<reg::demod::SPEC_INV>(|spec_inv| {
            spec_inv.set_spec_inv(false);
            spec_inv.set_en_aci(false);
        })
        .await?;

        // librtlsdr mentions clearing DDC shift registers starting at 0x16, but these
        // are not documented.
        //
        // they already cleared 0x16, 0x17, and pfset_iffreq starts is at 0x19, 0x1a,
        // 0x1b

        // clear ddc offset
        self.write_register_with::<reg::demod::UNK_DDC_OFFSET>(|ddc_offset| {
            ddc_offset.0 = 0;
        })
        .await?;

        // clear pset_iffreq (librtlsdr)
        self.write_register_with::<reg::demod::PSET_IFFREQ>(|pset_iffreq| {
            pset_iffreq.set_pset_iffreq(0)
        })
        .await?;

        // set filter
        self.write_register_with::<reg::demod::UNK_FIR_FILTER>(|fir_filter| {
            // todo
        })
        .await?;

        Ok(())
    }
}

/// # Arguments
///
/// - `f_if_d`: Intermediate frequency (IF) after sub-sampling
/// - `f_crystal`: Crystal frequency
pub fn pset_iffreq_from_hz(f_if_d: f32, f_crystal: f32) -> u32 {
    let f = -((f_if_d / f_crystal) * 4194304.0).floor();
    (f as i32).cast_unsigned() & 0x003fffff
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
pub enum InvalidFilter {
    #[error("FIR Filter coefficient out of range at index {index}")]
    CoefficientOutOfRange { index: usize },

    #[error("FIR Filter has incorrect length of {length} (instead of 16)")]
    InvalidLength { length: usize },
}

/// Linux uses
/// `\xca\xdc\xd7\xd8\xe0\xf2\x0e\x35\x06\x50\x9c\x0d\x71\x11\x14\x71\x74\x19\
/// x41\xa5`.
///
/// librtlsdr generates the bytes from proper filter coefficients:
///
/// ```c
/// /*
///  * FIR coefficients.
///  *
///  * The filter is running at XTal frequency. It is symmetric filter with 32
///  * coefficients. Only first 16 coefficients are specified, the other 16
///  * use the same values but in reversed order. The first coefficient in
///  * the array is the outer one, the last, the last is the inner one.
///  * First 8 coefficients are 8 bit signed integers, the next 8 coefficients
///  * are 12 bit signed integers. All coefficients have the same weight.
///  *
///  * Default FIR coefficients used for DAB/FM by the Windows driver,
///  * the DVB driver uses different ones
///  */
/// static const int fir_default[16] = {
/// 	-54, -36, -41, -40, -32, -14, 14, 53,	/* 8 bit signed */
/// 	101, 156, 215, 273, 327, 372, 404, 421	/* 12 bit signed */
/// };
/// ```
///
/// This is what it writes to memory (starting at 0x1c):
///
/// ```plain
/// │00000010│                         ┊             ca dc d7 d8 │        ┊    ××××│
/// │00000020│ e0 f2 0e 35 06 50 9c 0d ┊ 71 11 14 71 74 19 41 a5 │××•5•P×_┊q••qt•A×│
/// ```
///
/// So they both use the same filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirFilter {
    coefficients: [i16; 16],
}

impl FirFilter {
    pub const DEFAULT: Self = Self {
        coefficients: [
            -54, -36, -41, -40, -32, -14, 14, 53, 101, 156, 215, 273, 327, 372, 404, 421,
        ],
    };

    pub fn decode(buffer: &[u8; 20]) -> Self {
        let mut coefficients = [0; 16];

        for i in 0..8 {
            coefficients[i] = buffer[i].cast_signed().into();
        }

        for i in 0..4 {
            let x = u16::from(buffer[i * 3 + 8]);
            let y = u16::from(buffer[i * 3 + 9]);
            let z = u16::from(buffer[i * 3 + 10]);

            let mut a = (x << 4) | (y >> 4);
            let mut b = ((y & 0xf) << 8) | z;

            // sign-extend
            if a & 0x800 != 0 {
                a |= 0xf00;
            }
            if b & 0x800 != 0 {
                b |= 0xf00;
            }

            coefficients[i * 2 + 8] = a.cast_signed();
            coefficients[i * 2 + 9] = b.cast_signed();
        }

        Self { coefficients }
    }

    pub fn encode(&self, buffer: &mut [u8; 20]) {
        for i in 0..8 {
            buffer[i] = i8::try_from(self.coefficients[i]).unwrap().cast_unsigned();
        }

        // each iteration puts 2 i12 into 3 u8
        // input   fedcba987654 3210 fedcba98 76543210
        //         ----xxxxxxxx xxxx ----yyyy yyyyyyyy
        // output      76543210 7654     3210 76543210
        for i in 0..4 {
            let x = self.coefficients[i * 2 + 8].cast_unsigned();
            let y = self.coefficients[i * 2 + 9].cast_unsigned();

            buffer[i * 3 + 8] = (x >> 4).try_into().unwrap();
            buffer[i * 3 + 9] = (((x & 0x00f) << 4) | (y >> 8)).try_into().unwrap();
            buffer[i * 3 + 10] = (y & 0x0ff).try_into().unwrap();
        }
    }
}

impl Default for FirFilter {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl Deref for FirFilter {
    type Target = [i16];

    fn deref(&self) -> &Self::Target {
        &self.coefficients
    }
}

impl AsRef<[i16]> for FirFilter {
    fn as_ref(&self) -> &[i16] {
        &self.coefficients
    }
}

impl TryFrom<[i16; 16]> for FirFilter {
    type Error = InvalidFilter;

    fn try_from(value: [i16; 16]) -> Result<Self, Self::Error> {
        for i in 0..8 {
            if i8::try_from(value[i]).is_err() {
                return Err(InvalidFilter::CoefficientOutOfRange { index: i });
            }
        }

        for i in 0..4 {
            let x = value[i * 2 + 8].cast_unsigned();
            let y = value[i * 2 + 9].cast_unsigned();

            // the upper 4 bits must either be 0 or f depending on the sign of the i12.
            // we check if these bits correspond to the msb of the i12.
            if x & 0xf80 != 0xf8 && x & 0xf80 != 0 {
                return Err(InvalidFilter::CoefficientOutOfRange { index: i * 2 });
            }
            if y & 0xf80 != 0xf8 && y & 0xf80 != 0 {
                return Err(InvalidFilter::CoefficientOutOfRange { index: i * 2 + 1 });
            }
        }

        Ok(Self {
            coefficients: value,
        })
    }
}

impl TryFrom<&[i16]> for FirFilter {
    type Error = InvalidFilter;

    fn try_from(value: &[i16]) -> Result<Self, Self::Error> {
        <[i16; 16]>::try_from(value)
            .map_err(|_| {
                InvalidFilter::InvalidLength {
                    length: value.len(),
                }
            })?
            .try_into()
    }
}

#[cfg(test)]
mod tests {
    use crate::rtl2832u::{
        FirFilter,
        pset_iffreq_from_hz,
    };

    #[test]
    fn test_pset_iffreq_from_hz() {
        let pset_iffreq = pset_iffreq_from_hz(4.57 * 1000000.0, 28.8 * 1000000.0);
        assert_eq!(pset_iffreq, 0x0035d82e);
    }

    const ENCODED_FILTER: &[u8; 20] =
        b"\xca\xdc\xd7\xd8\xe0\xf2\x0e\x35\x06\x50\x9c\x0d\x71\x11\x14\x71\x74\x19\x41\xa5";

    #[test]
    fn fiter_encode() {
        let mut buffer = Default::default();
        FirFilter::DEFAULT.encode(&mut buffer);
        assert_eq!(&buffer, ENCODED_FILTER);
    }

    #[test]
    fn fiter_decode() {
        let filter = FirFilter::decode(ENCODED_FILTER);
        assert_eq!(filter, FirFilter::DEFAULT);
    }
}
