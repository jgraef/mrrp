//! I2C functions
//!
//! You can read and write to the I2C bus via [`read_i2c`](Rtl2832u::read_i2c),
//! [`read_i2c_register`](Rtl2832u::write_i2c),
//! [`write_i2c`](Rtl2832u::write_i2c), and
//! [`write_i2c_register`](Rtl2832u::write_i2c_register).
//!
//! The tuner chip is usually disconnected from the rest of the bus. It can be
//! enabled via the [`IIC_repeat`](SOFT_RST_IIC_REPEAT) flag.
//!
//! # Features
//!
//! If the `embedded-hal` feature is enabled,
//! [`embedded_hal_async::i2c::I2c`][1] is implemented for [`Rtl2832u`]. At the
//! time of writing the transaction functionality is not implemented. Note that
//! `embedded-hal` uses right-aligned addresses.
//!
//! [1]: https://docs.rs/embedded-hal-async/latest/embedded_hal_async/i2c/trait.I2c.html

use std::fmt::Debug;

use crate::rtl2832u::{
    Error,
    Rtl2832u,
    register::{
        Register,
        demod::SOFT_RST_IIC_REPEAT,
    },
};

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
    pub const fn from_left_aligned(address: u8) -> Self {
        if address & 1 != 0 {
            panic!("Address not left-aligned, or read-bit set");
        }

        Self(address)
    }

    /// Format used by e.g. `embedded_hal::i2c`
    pub const fn from_right_aligned(address: u8) -> Self {
        if address & 0x80 != 0 {
            panic!("Address not right-aligned, or read-bit set");
        }

        Self(address << 1)
    }

    /// Returns the "left-aligned" address
    pub fn left_aligned(&self) -> u8 {
        self.0
    }

    /// Returns the "right-aligned" address
    pub fn right_aligned(&self) -> u8 {
        self.0 >> 1
    }
}

impl Debug for I2cAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2cAddress(0x{:02x})", self.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct I2cRegister(pub u8);

impl Debug for I2cRegister {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "I2cRegister(0x{:02x})", self.0)
    }
}

impl Rtl2832u {
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
        i2c_register: I2cRegister,
        length: u16,
    ) -> Result<Vec<u8>, Error> {
        tracing::debug!(?i2c_address, ?i2c_register, length, "read I2C registers");

        self.write_i2c(i2c_address, &[i2c_register.0]).await?;
        let data = self.read_i2c(i2c_address, length).await?;

        tracing::debug!(
            ?i2c_address,
            ?i2c_register,
            length,
            ?data,
            "read I2C registers"
        );

        Ok(data)
    }

    /// Writes data to the I2C device
    pub async fn write_i2c_register(
        &mut self,
        i2c_address: I2cAddress,
        i2c_register: I2cRegister,
        data: &[u8],
    ) -> Result<(), Error> {
        tracing::debug!(?i2c_address, ?i2c_register, ?data, "write I2C registers");

        /*
        // take scratch buffer. can't borrow it because we call self.write
        let mut buffer = std::mem::take(&mut self.scratch_buffer);
        buffer.clear();
        buffer.extend(std::iter::once(i2c_register.0).chain(data.iter().copied()));

        self.write(Register::I2c { i2c_address }, &buffer).await?;

        // give scratch buffer back
        buffer.clear();
        self.scratch_buffer = buffer;

        Ok(())
        */
        todo!("write_i2c_register");
    }

    /// Enable the I2C repeater
    ///
    /// Connects the tuner to the I2C bus.
    pub async fn set_i2c_repeater(&mut self, on: bool) -> Result<(), Error> {
        // note: with the shadow map we could also just do a write_register_update now.
        // we think it's better to have a proper flag tracking this. then it still works
        // if we decide to disable shadow on this register

        if self.i2c_repeater_enabled != on {
            self.write_register_with::<SOFT_RST_IIC_REPEAT>(|iic_repeat| {
                iic_repeat.set_iic_repeat(on);
            })
            .await?;

            self.i2c_repeater_enabled = on;
        }

        Ok(())
    }

    pub fn i2c_repeater_enabled(&self) -> bool {
        self.i2c_repeater_enabled
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

#[derive(Clone, Copy)]
pub struct I2cReadProbe {
    pub register: I2cRegister,
    pub expected_value: u8,
}

impl I2cReadProbe {
    pub async fn probe(
        &self,
        rtl2832u: &mut Rtl2832u,
        i2c_address: I2cAddress,
    ) -> Result<bool, Error> {
        tracing::debug!(?i2c_address, register = ?self.register, expected_value = ?self.expected_value, "probing I2C");

        let result = rtl2832u
            .read_i2c_register(i2c_address, self.register, 1)
            .await;

        tracing::debug!(?result, "probe result");

        match result {
            Ok(data) => Ok(data[0] == self.expected_value),
            Err(Error::UsbTransfer(nusb::transfer::TransferError::Stall)) => {
                // no device at this I2C address
                Ok(false)
            }
            Err(error) => Err(error),
        }
    }
}
