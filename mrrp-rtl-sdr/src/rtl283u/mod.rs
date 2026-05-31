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
    collections::HashMap,
    time::Duration,
};

use crate::{
    Error,
    i2c::I2cRepeater,
    rtl283u::register::{
        self as reg,
        Register,
        RegisterAddress,
    },
};

#[derive(Debug)]
pub struct Rtl283u {
    usb_interface: nusb::Interface,
    control_timeout: Duration,
    dummy_registers: HashMap<Register, u32>,
}

impl Rtl283u {
    pub(crate) fn new(usb_interface: nusb::Interface, control_timeout: Duration) -> Self {
        Self {
            usb_interface,
            control_timeout,
            dummy_registers: HashMap::new(),
        }
    }

    pub fn i2c_repeater(&mut self) -> I2cRepeater<'_> {
        I2cRepeater::new(&mut self.usb_interface)
    }

    pub async fn read_register<R>(&mut self) -> Result<R, Error>
    where
        R: RegisterAddress + From<u32>,
    {
        // wish they didn't allocate
        let data = self
            .usb_interface
            .control_in(R::ADDRESS.control_in(4), self.control_timeout)
            .await?;

        let data: [u8; 4] = data.as_slice().try_into().map_err(|_| {
            Error::InvalidControlResponse {
                expected_length: 4,
                response_length: data.len(),
            }
        })?;

        let bits = u32::from_le_bytes(data);

        Ok(bits.into())
    }

    pub async fn write_register<R>(&mut self, value: R) -> Result<(), Error>
    where
        R: RegisterAddress,
        u32: From<R>,
    {
        /*let data = u32::from(value).to_le_bytes();

        self.usb_interface
            .control_out(R::ADDRESS.control_out(&data), self.control_timeout)
            .await?;*/

        self.dummy_registers.insert(R::ADDRESS, value.into());

        Ok(())
    }

    pub async fn test(&mut self) -> Result<(), Error> {
        let usb_sysctl = self.read_register::<reg::usb::SystemControl>().await?;
        tracing::debug!(?usb_sysctl);

        // endpoint A configuration
        let usb_epa_config = self.read_register::<reg::usb::EndpointAConfig>().await?;
        tracing::debug!(?usb_epa_config);

        // endpoint A control
        let usb_epa_control = self.read_register::<reg::usb::EndpointAControl>().await?;
        tracing::debug!(?usb_epa_control);

        Ok(())
    }

    pub async fn initialize_baseband(&mut self) -> Result<(), Error> {
        // enable DMA and enable full packet mode
        self.write_register(reg::usb::SystemControl::new(false, true, true))
            .await?;

        // set max packet size to 512
        self.write_register(reg::usb::EndpointAMaxPacketSize::new(0x200))
            .await?;

        todo!();
    }
}
