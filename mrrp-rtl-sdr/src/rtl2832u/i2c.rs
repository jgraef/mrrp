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

use std::{
    collections::HashSet,
    fmt::Debug,
    ops::{
        Deref,
        DerefMut,
    },
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            Ordering,
        },
    },
};

use parking_lot::Mutex;

use crate::rtl2832u::{
    Error,
    Rtl2832u,
    register::{
        Register,
        demod::SOFT_RST_IIC_REPEAT,
    },
    usb::UsbInterface,
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

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("The I2C device at {i2c_address:?} is already in use.")]
pub struct I2cDeviceBusy {
    pub i2c_address: I2cAddress,
}

#[derive(Debug)]
pub struct I2cDevice {
    usb_interface: UsbInterface,
    i2c_address: I2cAddress,
    shared: Arc<I2cShared>,
}

#[derive(Debug, Default)]
pub(super) struct I2cShared {
    repeater_enabled: AtomicBool,

    // note: a bitset would also be nice
    device_locks: Mutex<HashSet<I2cAddress>>,
}

impl I2cShared {
    fn repeater_enabled(&self) -> bool {
        self.repeater_enabled.load(Ordering::Relaxed)
    }
}

impl I2cDevice {
    pub fn address(&self) -> I2cAddress {
        self.i2c_address
    }

    /// Reads data from the I2C device.
    pub async fn read(&mut self, length: u16) -> Result<Vec<u8>, Error> {
        tracing::debug!(i2c_address = ?self.i2c_address, i2c_repeater_enabled = ?self.shared.repeater_enabled(), ?length, "reading I2C");

        self.usb_interface
            .read(
                Register::I2c {
                    i2c_address: self.i2c_address,
                },
                length,
            )
            .await
    }

    /// Writes data to the I2C device.
    pub async fn write(&mut self, data: &[u8]) -> Result<(), Error> {
        tracing::debug!(i2c_address = ?self.i2c_address, i2c_repeater_enabled = ?self.shared.repeater_enabled(), ?data, "writing I2C");

        self.usb_interface
            .write(
                Register::I2c {
                    i2c_address: self.i2c_address,
                },
                data,
            )
            .await
    }
}

impl Drop for I2cDevice {
    fn drop(&mut self) {
        let mut guard = self.shared.device_locks.lock();
        guard.remove(&self.i2c_address);
    }
}

impl Rtl2832u {
    /// Opens an I2C device handle
    ///
    /// This does not check if the device exists. This only checks that the
    /// device at that address is not already in use (i.e. by another call
    /// to this method).
    ///
    /// The returned device handle can be used independently of the
    /// [`Rtl2832u`].
    ///
    /// The device will be made available again when the [`I2cDevice`] handle is
    /// dropped.
    ///
    /// # I2C repeater
    ///
    /// Note that this does not ensure that the I2C repeater is enabled, if you
    /// need that for your device (i.e. a tuner). You'll need to do that via
    /// [`set_i2c_repeater`](Self::set_i2c_repeater) or
    /// [`with_i2c_repeater`](Self::with_i2c_repeater).
    pub fn try_open_i2c(&mut self, i2c_address: I2cAddress) -> Result<I2cDevice, I2cDeviceBusy> {
        let mut guard = self.i2c_shared.device_locks.lock();

        if guard.insert(i2c_address) {
            Ok(I2cDevice {
                usb_interface: self.usb_interface.clone(),
                i2c_address,
                shared: self.i2c_shared.clone(),
            })
        }
        else {
            Err(I2cDeviceBusy { i2c_address })
        }
    }

    /// Enable the I2C repeater
    ///
    /// Connects the tuner to the I2C bus.
    pub(super) async fn set_i2c_repeater(&mut self, on: bool) -> Result<(), Error> {
        // note: with the shadow map we could also just do a write_register_update now.
        // we think it's better to have a proper flag tracking this. then it still works
        // if we decide to disable shadow on this register

        if self.i2c_shared.repeater_enabled.swap(on, Ordering::Relaxed) != on {
            self.write_register_update::<SOFT_RST_IIC_REPEAT>(|iic_repeat| {
                iic_repeat.set_iic_repeat(on);
            })
            .await?;
        }

        Ok(())
    }

    pub async fn enable_i2c_repeater(&mut self) -> Result<I2cRepeaterGuard<'_>, Error> {
        self.set_i2c_repeater(true).await?;

        Ok(I2cRepeaterGuard { rtl2832u: self })
    }

    #[inline(always)]
    pub fn i2c_repeater_enabled(&self) -> bool {
        self.i2c_shared.repeater_enabled()
    }
}

#[derive(Debug)]
#[must_use]
pub struct I2cRepeaterGuard<'a> {
    rtl2832u: &'a mut Rtl2832u,
}

impl<'a> I2cRepeaterGuard<'a> {
    pub async fn disable(self) -> Result<(), Error> {
        self.rtl2832u.set_i2c_repeater(false).await?;
        Ok(())
    }
}

impl<'a> Deref for I2cRepeaterGuard<'a> {
    type Target = Rtl2832u;

    fn deref(&self) -> &Self::Target {
        self.rtl2832u
    }
}

impl<'a> DerefMut for I2cRepeaterGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.rtl2832u
    }
}

impl<'a> Drop for I2cRepeaterGuard<'a> {
    fn drop(&mut self) {
        if self.rtl2832u.i2c_repeater_enabled() {
            tracing::warn!("Dropped I2cRepeaterGuard without disabling repeater");
        }
    }
}
