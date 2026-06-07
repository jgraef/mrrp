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
pub mod gpio;
pub mod i2c;
pub mod register;
pub(crate) mod usb;

use std::{
    collections::HashSet,
    fmt::Debug,
    sync::Arc,
    time::Duration,
};

use bitfield::BitRangeMut;
use parking_lot::Mutex;

pub use crate::rtl2832u::usb::Reader;
use crate::rtl2832u::{
    filter::FirFilter,
    i2c::I2cAddress,
    register::{
        self as reg,
        Bits,
        Register,
        RegisterValue,
        shadow::{
            self,
            ShadowMap,
        },
    },
    usb::UsbInterface,
};

pub const DEFAULT_CRYSTAL_FREQUENCY: u32 = 28800000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Usb(#[from] nusb::Error),

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

/// Options for [`Rtl2832u`]
#[derive(Clone, Debug)]
pub struct OpenOptions {
    /// Detach the kernel driver before claiming the USB interface.
    ///
    /// This only works on Linux, and is ignored on other platforms.
    pub detach_kernel_driver: bool,

    /// Timeout for a control operation. Default is 5 seconds.
    pub control_timeout: Duration,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            detach_kernel_driver: false,
            control_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResetOptions {
    pub stop_data_stream: bool,
    pub poweroff_demod: bool,
    pub reset_gpio: bool,
}

impl Default for ResetOptions {
    fn default() -> Self {
        Self {
            stop_data_stream: true,
            poweroff_demod: true,
            reset_gpio: true,
        }
    }
}

/// Low-level interface to the `RTL2832U` chip via USB.
#[derive(Debug)]
pub struct Rtl2832u {
    usb_interface: UsbInterface,
    i2c_repeater_enabled: bool,
    shadow_map: ShadowMap,

    // note: a bitset would also be nice
    i2c_device_locks: Arc<Mutex<HashSet<I2cAddress>>>,
}

impl Rtl2832u {
    /// Creates a RTK2832U interface from a USB interface.
    ///
    /// This method doesn't initialize anything. It actually doesn't interact
    /// with the device at all.
    pub fn new(usb_interface: nusb::Interface, control_timeout: Duration) -> Self {
        Self {
            usb_interface: UsbInterface::new(usb_interface, control_timeout),
            i2c_repeater_enabled: false,
            shadow_map: ShadowMap::default(),
            i2c_device_locks: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Read raw registers
    ///
    /// This is a low-level function that takes a dynamic register address (and
    /// length) and returns the raw bytes from these registers. See
    /// [`read_register`](Self::read_register) for a statically typed variant.
    pub async fn read(&mut self, address: Register, length: u16) -> Result<Vec<u8>, Error> {
        self.usb_interface.read(address, length).await
    }

    /// Write raw registers
    ///
    /// This is a low-level function that takes a dynamic register address and
    /// writes raw bytes to it. See [`write_register`](Self::write_register),
    /// [`write_register_with`](Self::write_register_with),
    /// and [`write_register_update`](Self::write_register_update) for
    /// statically typed variants.
    pub async fn write(&mut self, address: Register, data: &[u8]) -> Result<(), Error> {
        self.usb_interface.write(address, data).await
    }

    /// Read a statically typed [`Register`]
    pub async fn read_register<R>(&mut self) -> Result<R, Error>
    where
        R: RegisterValue + shadow::ShadowRegister,
    {
        if let Some(value) = R::shadow_read(&self.shadow_map) {
            Ok(*value)
        }
        else {
            self.read_register_no_shadow().await
        }
    }

    /// Read a statically typed [`Register`]
    ///
    /// This method variant will always read from the device and ignore any
    /// cached values in the shadow map. It will write the read value to the
    /// shadow map though.
    pub async fn read_register_no_shadow<R>(&mut self) -> Result<R, Error>
    where
        R: RegisterValue + shadow::ShadowRegister,
    {
        let data = self
            .read(R::ADDRESS, <R::Bits as register::Bits>::LENGTH)
            .await?;

        let bits = <R::Bits as register::Bits>::from_bytes(&data);
        let value = R::from_bits(bits);

        tracing::debug!(address = ?R::ADDRESS, ?value, "read register");

        value.shadow_write(&mut self.shadow_map);

        Ok(value)
    }

    /// Write a statically typed [`Register`]
    pub async fn write_register<R>(&mut self, value: R) -> Result<(), Error>
    where
        R: RegisterValue + shadow::ShadowRegister,
    {
        tracing::debug!(address = ?R::ADDRESS, ?value, "writing register");

        let bits = value.as_bits();
        let data = bits.into_bytes();

        self.write(R::ADDRESS, data.as_ref()).await?;

        value.shadow_write(&mut self.shadow_map);

        Ok(())
    }

    /// Read a statically typed [`Register`]
    ///
    /// This is a convenience wrapper around
    /// [`write_register`](Self::write_register) that initializes
    /// a buffer with its [`Default`] value (i.e. all zeros), and calls your
    /// closure with it.
    ///
    /// # Example
    ///
    /// ```
    /// # use mrrp_rtl_sdr::rtl2832u::{Rtl2832u, Error, register::sys::DEMOD_CTL};
    /// # async fn example() -> Result<(), Error> {
    /// # let rtl2832u: Rtl2832u = todo!();
    /// rtl2832u
    ///     .write_register_with::<DEMOD_CTL>(|demod_ctl| {
    ///         demod_ctl.set_pll_enable(true);
    ///         demod_ctl.set_adc_i_enable(true);
    ///         demod_ctl.set_hardware_reset(true);
    ///         demod_ctl.set_adc_q_enable(true);
    ///     })
    ///     .await?;
    /// # }
    /// ```
    pub async fn write_register_with<R>(&mut self, f: impl FnOnce(&mut R)) -> Result<(), Error>
    where
        R: RegisterValue + shadow::ShadowRegister,
    {
        let mut value = Default::default();
        f(&mut value);

        self.write_register(value).await
    }

    pub async fn write_register_update<R>(&mut self, f: impl FnOnce(&mut R)) -> Result<(), Error>
    where
        R: RegisterValue + shadow::ShadowRegister,
    {
        let current_value = self.read_register::<R>().await?;

        let mut new_value = current_value.clone();
        f(&mut new_value);

        if new_value != current_value {
            self.write_register(new_value).await?;
        }

        Ok(())
    }

    pub async fn initialize(&mut self, fir_filter: &FirFilter) -> Result<(), Error> {
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
        self.write_register_with::<reg::demod::SPEC_INV_EN_ACI>(|spec_inv| {
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
        self.write_register_with::<reg::demod::ZERO_IF_IQ_COMP>(|dc_cancel| {
            // Zero-IF mode
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

    pub async fn poweron_demod(&mut self) -> Result<(), Error> {
        self.write_register_with::<reg::sys::DEMOD_CTL>(|demod_ctl| {
            demod_ctl.set_pll_enable(true);
            demod_ctl.set_hardware_reset(true);
        })
        .await?;
        Ok(())
    }

    pub async fn reset(&mut self, options: ResetOptions) -> Result<(), Error> {
        tracing::debug!("resetting device");

        // turn off I2C repeater, if it is on.
        self.set_i2c_repeater(false).await?;

        if options.stop_data_stream {
            self.stop_data_stream().await?;
        }

        if options.poweroff_demod {
            // `rtlsdr_deinit_baseband` sets DEMOD_CTL to 0x20, meaning PLL, ADC I/Q are
            // disabled, but the reset flag is inverted, so it's released. I think the PLL
            // enable actually determines if the demod chip is powered.

            // disable demod PLL, ADC I and Q
            self.write_register_with::<reg::sys::DEMOD_CTL>(|demod_ctl| {
                demod_ctl.set_hardware_reset(true);
            })
            .await?;
        }

        if options.reset_gpio {
            // disable GPIO outputs
            //
            // reset state from datasheet
            self.write_register::<reg::sys::GPO>(0x18.into()).await?;
            self.write_register::<reg::sys::GPOE>(0x19.into()).await?;
            self.write_register::<reg::sys::GPD>(0x0e.into()).await?;
        }

        Ok(())
    }

    pub async fn start_data_stream(&mut self) -> Result<(), Error> {
        tracing::debug!("start data stream");

        self.write_register_update::<reg::usb::EPA_CTL>(|epa_ctl| {
            epa_ctl.set_stall_endpoint(false);
            epa_ctl.set_fifo_reset(false);
        })
        .await?;
        Ok(())
    }

    pub async fn stop_data_stream(&mut self) -> Result<(), Error> {
        tracing::debug!("stop data stream");

        self.write_register_update::<reg::usb::EPA_CTL>(|epa_ctl| {
            epa_ctl.set_stall_endpoint(true);
            epa_ctl.set_fifo_reset(true);
        })
        .await?;
        Ok(())
    }

    pub fn reader(&mut self, buffer_size: usize) -> Result<Reader, Error> {
        self.usb_interface.data_endpoint_reader(buffer_size)
    }

    pub async fn set_if_mode(&mut self, if_mode: IfMode) -> Result<(), Error> {
        let is_zero_if = matches!(if_mode, IfMode::ZeroIf);

        // this has already been initialized, so it's in the shadow map
        self.write_register_update::<reg::demod::ZERO_IF_IQ_COMP>(|zero_if| {
            zero_if.set_en_bbin(is_zero_if);
        })
        .await?;

        // enable in-phase ADC input
        //
        // this has not been touched before, but we know the lower nibble has to be
        // 0x0d. should be use `write_register_update` anyway? would be nice if
        // we knew what that lower nibble actually encodes.
        self.write_register_with::<reg::demod::ADC_ENABLE>(|adc_enable| {
            adc_enable.set_en_i(true);
            adc_enable.set_en_q(is_zero_if);

            // idk what this is. it's this at startup and librtlsdr sets this, whenever they
            // toggle ADC inputs
            adc_enable.set_bit_range(3, 0, 0xd);
        })
        .await?;

        Ok(())
    }

    pub async fn set_if_frequency(
        &mut self,
        frequency: f32,
        crystal_frequency: f32,
    ) -> Result<(), Error> {
        let value = pset_iffreq_from_hz(frequency, crystal_frequency);

        // todo: we made the pset_iffreq register 32bit for convenience, but there might
        // be something important in the upper bits (DDC offset?). these bits
        // are also not 0 at startup, so just to be sure, we'll to an update here and
        // only change bits that we want changed.

        self.write_register_update::<reg::demod::PSET_IFFREQ>(|pset_iffreq| {
            pset_iffreq.set_pset_iffreq(value);
        })
        .await?;

        Ok(())
    }

    pub async fn set_sample_frequency_correction(&mut self, ppm: i16) -> Result<(), Error> {
        self.write_register_with::<reg::demod::SAMP_FREQ_CORR>(|samp_freq_corr| {
            samp_freq_corr.set_samp_freq_corr(ppm);
        })
        .await
    }

    /// Enables or disables spectrum inversion
    pub async fn enable_spectrum_inversion(&mut self, enable: bool) -> Result<(), Error> {
        self.write_register_update::<reg::demod::SPEC_INV_EN_ACI>(|spec_inv| {
            spec_inv.set_spec_inv(enable);
        })
        .await?;

        Ok(())
    }

    /// Set sample rate
    ///
    /// This will set the sample rate to the closest possible value to the
    /// provided sample rate, and return the actual sample rate set in the
    /// device.
    pub async fn set_sample_rate(
        &mut self,
        sample_rate: f32,
        crystal_frequency: f32,
    ) -> Result<f32, Error> {
        // librtlsdr ensures the sample_rate is valid. is there something in the
        // datasheet about this?
        //
        // if (sample_rate <= 225000)
        //    || (sample_rate > 3200000)
        //    || ((sample_rate > 300000) && (sample_rate <= 900000))
        //{}

        let rsamp_ratio = rsamp_ratio_from_hz(sample_rate, crystal_frequency);
        let actual_sample_rate = rsamp_ratio_to_hz(rsamp_ratio, crystal_frequency);

        self.write_register_update::<reg::demod::CFREQ_OFF_RATIO_RSAMP_RATIO>(|register| {
            register.set_rsamp_ratio(rsamp_ratio);
        })
        .await?;

        // reset demod (soft reset)
        self.write_register_update::<reg::demod::SOFT_RST_IIC_REPEAT>(|soft_rst| {
            soft_rst.set_soft_rst(true);
        })
        .await?;
        self.write_register_update::<reg::demod::SOFT_RST_IIC_REPEAT>(|soft_rst| {
            soft_rst.set_soft_rst(false);
        })
        .await?;

        Ok(actual_sample_rate)
    }

    pub async fn get_sample_rate(&mut self, crystal_frequency: f32) -> Result<f32, Error> {
        let rsamp_ratio = self
            .read_register::<reg::demod::CFREQ_OFF_RATIO_RSAMP_RATIO>()
            .await?;
        Ok(rsamp_ratio_to_hz(
            rsamp_ratio.rsamp_ratio(),
            crystal_frequency,
        ))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IfMode {
    If,
    ZeroIf,
}

/// Calculate [`pset_iffreq`](reg::demod::PSET_IFFREQ) value from intermediate
/// frequency and crystal frequency.
///
/// # Arguments
///
/// - `f_if_d`: Intermediate frequency (IF) after sub-sampling
/// - `f_crystal`: Crystal frequency
pub fn pset_iffreq_from_hz(f_if_d: f32, f_crystal: f32) -> u32 {
    // librtlsdr does this with u32's but we're pretty sure that overflows.
    //
    // example: r82xx if is 3570000, multiplied by 4194304 is at least 44 bits. the
    // division then would yield incorrect results, no?
    //
    // and since we're multiplying by 4194304 (2**22) floating-point arithmetic is
    // well-suited here.

    let f = -(f_if_d * 4194304.0 / f_crystal).floor();
    (f as i32).cast_unsigned() & 0x003f_ffff
}

pub fn rsamp_ratio_from_hz(f_symbol: f32, f_crystal: f32) -> u32 {
    let r = (f_crystal * 4194304.0 / f_symbol).floor();
    (r as u32) & 0x03ff_ffff
}

pub fn rsamp_ratio_to_hz(rsamp_ratio: u32, f_crystal: f32) -> f32 {
    f_crystal * 4194304.0 / rsamp_ratio as f32
}

#[cfg(test)]
mod tests {
    use crate::rtl2832u::{
        FirFilter,
        pset_iffreq_from_hz,
        rsamp_ratio_from_hz,
    };

    #[test]
    fn test_pset_iffreq_from_hz() {
        assert_eq!(pset_iffreq_from_hz(4570000.0, 28800000.0), 0x0035_d82e);
        assert_eq!(pset_iffreq_from_hz(36167000.0, 28800000.0), 0x002f_a0ff);
        assert_eq!(pset_iffreq_from_hz(36125000.0, 28800000.0), 0x002f_b8e4);
        assert_eq!(pset_iffreq_from_hz(0.0, 28800000.0), 0);
    }

    #[test]
    fn test_rsamp_ratio_from_hz() {
        assert_eq!(
            rsamp_ratio_from_hz(64.0 * 1000000.0 / 7.0, 28800000.0),
            // tried debugging the discrepancy and we're pretty sure it's because of
            // rounding of f_symbol
            0x00c9_9999 + 1
        );
        assert_eq!(
            rsamp_ratio_from_hz(8.0 * 1000000.0, 28800000.0),
            0x00e6_6666
        );
        assert_eq!(
            rsamp_ratio_from_hz(48.0 * 1000000.0 / 7.0, 28800000.0),
            // note that this has one more c than the datasheet. probably a typo
            0x010c_cccc
        );
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
