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

pub mod filter;
pub mod register;

use std::{
    fmt::Debug,
    time::Duration,
};

use crate::rtl2832u::{
    filter::FirFilter,
    register::{
        self as reg,
        Bits,
        Register,
        RegisterValue,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    UsbTransfer(#[from] nusb::transfer::TransferError),

    #[error(
        "Invalid control response: expected {expected_length} bytes, but received {response_length} bytes."
    )]
    InvalidControlResponse {
        expected_length: u16,
        response_length: usize,
    },
}

#[derive(Debug)]
pub struct Rtl2832u {
    usb_interface: nusb::Interface,
    control_timeout: Duration,
    scratch_buffer: Vec<u8>,
    i2c_repeater_enabled: bool,
}

impl Rtl2832u {
    pub(crate) fn new(usb_interface: nusb::Interface, control_timeout: Duration) -> Self {
        Self {
            usb_interface,
            control_timeout,
            scratch_buffer: vec![],
            i2c_repeater_enabled: false,
        }
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

    pub async fn initialize(&mut self) -> Result<(), Error> {
        // todo: these should be options that are passed in
        let fir_filter = &FirFilter::DEFAULT;

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
        self.write_register(reg::demod::UNK_FIR_FILTER::from_filter(fir_filter))
            .await?;

        // disable dagc, "enable SDR mode"???
        self.write_register_with::<reg::demod::UNK_DAGC>(|unk_dagc| {
            unk_dagc.set_enable_dagc(false);
            // todo: figure out what they do
            unk_dagc.set_unk_0(true);
            unk_dagc.set_unk_2(true);
        })
        .await?;

        // configure FSM
        self.write_register(reg::demod::UNK_FSM(0x0ff0)).await?;

        // disable DAGC, librtlsdr says this has no effect
        self.write_register_with::<reg::demod::EN_DAGC>(|en_dagc| {
            en_dagc.set_endagc(false);
        })
        .await?;

        // disable RF and IF AGC loop
        self.write_register_with::<reg::demod::LOOP_GAIN2_3_0_AAGC_HOLD_EN_RF_AGC_EN_IF_AGC>(
            |en_agc| {
                en_agc.set_en_rf_agc(false);
                en_agc.set_en_if_agc(false);
            },
        )
        .await?;

        // disable PID (packet identifier) filter
        self.write_register_with::<reg::demod::PID_CTL>(|pid_ctl| {
            // we think we need to turn off PID filter output and set the mode to accept
            // rejected and error packets
            pid_ctl.set_err_pass(true);
            pid_ctl.set_mode(true);
            pid_ctl.set_enable(false);
        })
        .await?;

        // set I/Q ADC data path
        self.write_register_with::<reg::demod::OPT_ADC_IQ_MPEG_IO_OPT_2_2>(|opt_adc| {
            opt_adc.set_opt_adc_iq(0);
            // librtlsdr and linux sdr set this. don't know what it does
            opt_adc.set_mpeg_io_opt_2_2(true);
        })
        .await?;

        // zero-IF, DC cancellation,
        self.write_register_with::<reg::demod::DC_CANCEL>(|dc_cancel| {
            // this is disabled in rtl_test. i think this is only necessary for low
            // frequencies.
            //
            // this is disabled later in rtlsdr_open when a r828d or r820t is detected
            dc_cancel.set_en_bbin(true);

            dc_cancel.set_en_dc_est(true);
            dc_cancel.set_en_iq_comp(true);
            dc_cancel.set_en_iq_est(true);
        })
        .await?;

        // librtlsdr comments this as disabling TP_CK0. this pin is mentioned in the
        // datasheet but nothing else on it.
        //
        // linux dvbt has a register layout with some bits, but it's not clear what
        // they're about
        //
        // linux sdr just sets them during e4k tuner setup
        self.write_register_with::<reg::demod::REG_MON_REG_MONSEL_REG_GPE>(|reg| {
            reg.set_reg_mon(0b11);
            reg.set_reg_gpe(true);
        })
        .await?;

        Ok(())
    }

    pub async fn reset(&mut self) -> Result<(), Error> {
        tracing::debug!("resetting device");

        // todo: reset tuner

        // `rtlsdr_deinit_baseband` sets DEMOD_CTL to 0x20, meaning PLL, ADC I/Q are
        // disabled, but the reset flag is inverted, so it's released. I think the PLL
        // enable actually determines if the demod chip is powered.

        // disable demod PLL, ADC I and Q
        self.write_register_with::<reg::sys::DEMOD_CTL>(|demod_ctl| {
            demod_ctl.set_hardware_reset(true);
        })
        .await?;

        Ok(())
    }

    /// Reads data from the I2C device.
    pub async fn read_i2c(
        &mut self,
        i2c_address: I2cAddress,
        length: u16,
    ) -> Result<Vec<u8>, Error> {
        //        self.write(Register::I2c { i2c_address }, &[i2c_register])
        //            .await?;

        self.read(Register::I2c { i2c_address }, length).await
    }

    /// Writes data to the I2C device.
    pub async fn write_i2c(&mut self, i2c_address: I2cAddress, data: &[u8]) -> Result<(), Error> {
        self.write(Register::I2c { i2c_address }, data).await
    }

    /// Reads a register from the I2C device.
    ///
    /// First writes the register address to the device, then reads back data.
    pub async fn read_i2c_register(
        &mut self,
        i2c_address: I2cAddress,
        i2c_register: u8,
        length: u16,
    ) -> Result<Vec<u8>, Error> {
        self.write_i2c(i2c_address, &[i2c_register]).await?;
        self.read_i2c(i2c_address, length).await
    }

    /// Writes data to the I2C device
    pub async fn write_i2c_register(
        &mut self,
        i2c_address: I2cAddress,
        data: &[u8],
    ) -> Result<(), Error> {
        // take scratch buffer. can't borrow it because we call self.write
        let mut buffer = std::mem::take(&mut self.scratch_buffer);
        buffer.clear();
        buffer.extend(std::iter::once(i2c_address.0).chain(data.iter().copied()));

        self.write(Register::I2c { i2c_address }, &buffer).await?;

        // give scratch buffer back
        buffer.clear();
        self.scratch_buffer = buffer;

        Ok(())
    }

    pub async fn set_i2c_repeater(&mut self, on: bool) -> Result<(), Error> {
        if self.i2c_repeater_enabled != on {
            self.write_register_with::<reg::demod::SOFT_RST_IIC_REPEAT>(|iic_repeat| {
                iic_repeat.set_iic_repeat(on);
            })
            .await?;

            self.i2c_repeater_enabled = on;
        }

        Ok(())
    }

    pub async fn with_i2c_repeater<R, E>(
        &mut self,
        mut f: impl AsyncFnMut(&mut Self) -> Result<R, E>,
    ) -> Result<R, E>
    where
        E: From<Error>,
    {
        self.set_i2c_repeater(true).await?;

        let output = f(self).await;

        let disable_result = self.set_i2c_repeater(false).await;

        if output.is_ok()
            && let Err(error) = disable_result
        {
            Err(error.into())
        }
        else {
            output
        }
    }
}

/// I2C address
///
/// This address is in the format the RTL2832U expects, i.e. it is left-aligned.
/// This means the 7 bits of the address are in the 7 MSB bits, while the 0th
/// bit is 0.
///
/// **Be careful** to use the right addressing scheme, or you could potentially
/// write to the wrong device, e.g. the EEPROM.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct I2cAddress(u8);

impl I2cAddress {
    /// This is the format the RTL2832U expects.
    pub fn from_left_aligned(address: u8) -> Self {
        assert_eq!(
            address & 1,
            0,
            "Address not left-aligned, or read-bit set: 0x{address:02x}"
        );

        Self(address)
    }

    /// Format used by e.g. `embedded_hal::i2c`
    pub fn from_right_aligned(address: u8) -> Self {
        assert_eq!(
            address & 0x80,
            0,
            "Address not right-aligned, or read-bit set: 0x{address:02x}"
        );

        Self(address << 1)
    }

    /// Returns the "left-aligned" address
    pub fn left_aligned(&self) -> u8 {
        self.0
    }
}

impl Debug for I2cAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{:02x}", self.0)
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

#[cfg(feature = "embedded-hal")]
impl embedded_hal_async::i2c::Error for Error {
    fn kind(&self) -> embedded_hal_async::i2c::ErrorKind {
        // todo: can we determine the cause of an error?
        embedded_hal_async::i2c::ErrorKind::Other
    }
}

#[cfg(feature = "embedded-hal")]
impl embedded_hal_async::i2c::ErrorType for Rtl2832u {
    type Error = Error;
}

#[cfg(feature = "embedded-hal")]
impl embedded_hal_async::i2c::I2c for Rtl2832u {
    /// # TODO
    ///
    /// Not supported. We need to check if we can uphold the transaction
    /// contract required by `embedded_hal`.
    async fn transaction(
        &mut self,
        address: u8,
        operations: &mut [embedded_hal_async::i2c::Operation<'_>],
    ) -> Result<(), Self::Error> {
        let _ = (address, operations);
        todo!(
            "we need to check if we can uphold the transaction contract required by embedded_hal"
        );
    }

    async fn read(&mut self, address: u8, read: &mut [u8]) -> Result<(), Self::Error> {
        let data = self
            .read_i2c(
                I2cAddress::from_right_aligned(address),
                read.len().try_into().unwrap(),
            )
            .await?;
        read.copy_from_slice(&data);
        Ok(())
    }

    async fn write(&mut self, address: u8, write: &[u8]) -> Result<(), Self::Error> {
        self.write_i2c(I2cAddress::from_right_aligned(address), write)
            .await?;
        Ok(())
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
