//! Low-level interface for the RTL2832U
//!
//! [Datasheet][1], [`librtlsdr` (blog)][2]
//!
//! # Note
//!
//! When porting from `librtlsdr` it can be very confusing, because it looks
//! like they have their byteorder mixed up. They're writing registers a
//! big-endian, while the RTL232U uses little endian.
//!
//! [1]: https://homepages.uni-regensburg.de/~erc24492/SDR/Data_rtl2832u.pdf
//! [2]: https://github.com/rtlsdrblog/rtl-sdr-blog/blob/master/src/librtlsdr.c

pub mod register;

use std::{
    fmt::Debug,
    time::Duration,
};

use crate::{
    Error,
    i2c::I2cRepeater,
    rtl2832u::register::{
        self as reg,
        Bits,
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

    pub async fn read_register<R>(&mut self) -> Result<R, Error>
    where
        R: RegisterValue + Debug,
    {
        let request = R::ADDRESS.control_in(<R::Bits as register::Bits>::LENGTH);

        tracing::debug!(?request);

        // wish they didn't allocate
        let data = self
            .usb_interface
            .control_in(request, self.control_timeout)
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
        let bits = value.as_bits();

        tracing::debug!(address = ?R::ADDRESS, ?value, "writing register");

        let data = bits.into_bytes();

        self.usb_interface
            .control_out(R::ADDRESS.control_out(data.as_ref()), self.control_timeout)
            .await?;

        Ok(())
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

        let _i2c_repeat = self
            .read_register::<reg::demod::SOFT_RST_IIC_REPEAT>()
            .await?;

        // reset demod
        let mut iic_repeat = reg::demod::SOFT_RST_IIC_REPEAT(0x10);
        iic_repeat.set_soft_rst(true);
        self.write_register(iic_repeat).await?;
        iic_repeat.set_soft_rst(false);
        self.write_register(iic_repeat).await?;

        Ok(())
    }
}
