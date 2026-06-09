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
    sync::Arc,
};

use parking_lot::Mutex;

use crate::rtl2832u::{
    Error,
    Rtl2832u,
    Shared,
    Transaction,
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
#[derive(Clone, Copy, derive_more::Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct I2cAddress(#[debug("0x{_0:02x}")] u8);

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

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("The I2C device at {i2c_address:?} is already in use.")]
pub struct I2cDeviceBusy {
    pub i2c_address: I2cAddress,
}

#[derive(Debug, Default)]
pub(super) struct I2cState {
    /// Keeps track which I2C device is currently in use, meaning a
    /// [`I2cDevice`] exists for it.
    ///
    /// This is a [`parking_lot::Mutex`], because we don't need to hold this
    /// lock across an await point
    ///
    /// Note: A bitset would also work
    device_locks: Mutex<HashSet<I2cAddress>>,
}

impl I2cState {
    fn try_lock_device(&self, i2c_address: I2cAddress) -> bool {
        let mut guard = self.device_locks.lock();
        guard.insert(i2c_address)
    }

    fn unlock_device(&self, i2c_address: I2cAddress) {
        let mut guard = self.device_locks.lock();
        let was_present = guard.remove(&i2c_address);
        assert!(
            was_present,
            "Tried to unlock device, that is not locked: {i2c_address:?}"
        )
    }
}

#[derive(Debug)]
pub struct I2cDevice {
    shared: Arc<Shared>,
    i2c_address: I2cAddress,
    needs_repeater: bool,
}

impl I2cDevice {
    pub fn address(&self) -> I2cAddress {
        self.i2c_address
    }

    pub fn needs_repeater(&self) -> bool {
        self.needs_repeater
    }

    pub fn with_repeater(mut self) -> Self {
        self.needs_repeater = true;
        self
    }

    /// Reads data from the I2C device.
    pub async fn read(&mut self, length: u16) -> Result<Vec<u8>, Error> {
        tracing::debug!(i2c_address = ?self.i2c_address, ?length, "reading I2C");

        self.shared
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
        tracing::debug!(i2c_address = ?self.i2c_address, ?data, "writing I2C");

        self.shared
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
        self.shared.i2c_state.unlock_device(self.i2c_address);
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
    /// The `needs_repeater` bool controls whether this device needs the I2C
    /// repeater enabled. [`I2cTransaction`]s will be synchronized such that
    /// only devices are accessed in parallel that need it, or don't. This
    /// basically separates all device accesses in two groups, which can share
    /// the bus between the own group, but not with the other group.
    ///
    /// This will also ensure the repeater is actually enabled or disabled when
    /// the transaction begins.
    pub fn try_open_i2c(&self, i2c_address: I2cAddress) -> Result<I2cDevice, I2cDeviceBusy> {
        if self.shared.i2c_state.try_lock_device(i2c_address) {
            Ok(I2cDevice {
                shared: self.shared.clone(),
                i2c_address,
                needs_repeater: false,
            })
        }
        else {
            Err(I2cDeviceBusy { i2c_address })
        }
    }
}

impl<'a> Transaction<'a> {
    /// Enable the I2C repeater
    ///
    /// Connects the tuner to the I2C bus.
    ///
    /// This is a low-level function and is not synchronized with any
    /// [`I2cDevices`]. It is only pub in the [`rtl2832u`](super) module, so
    /// that it can be disabled when the device is reset. Otherwise this method
    /// is used when acquiring a [`I2cRepeaterGuard`] and when calling
    /// [`I2cRepeaterGuard::disable`].
    pub(super) async fn set_i2c_repeater(&mut self, on: bool) -> Result<(), Error> {
        self.write_register_update::<SOFT_RST_IIC_REPEAT>(|iic_repeat| {
            iic_repeat.set_iic_repeat(on);
        })
        .await?;

        Ok(())
    }

    /// Return if the I2C repeater is enabled
    ///
    /// This only reads the shadow map and never reads from the device.
    ///
    /// In the rare case that the `SOFT_RST_IIC_REPEAT` register value is
    /// unknown, `false` is returned.
    pub fn i2c_repeater_enabled(&self) -> bool {
        self.shadow_map_guard
            .demod
            .SOFT_RST_IIC_REPEAT
            .map_or(false, |iic_repeat| iic_repeat.iic_repeat())
    }

    /// Enables the I2C repeater and returns a guard that allows you to disable
    /// it again.
    pub async fn enable_i2c_repeater(&mut self) -> Result<I2cRepeaterGuard<'a, '_>, Error> {
        self.set_i2c_repeater(true).await?;

        Ok(I2cRepeaterGuard { transaction: self })
    }
}

#[derive(Debug)]
pub struct I2cRepeaterGuard<'rtl, 'tx> {
    transaction: &'tx mut Transaction<'rtl>,
}

impl<'rtl, 'tx> I2cRepeaterGuard<'rtl, 'tx> {
    pub async fn disable(self) -> Result<(), Error> {
        self.transaction.set_i2c_repeater(false).await?;
        Ok(())
    }
}

impl<'rtl, 'tx> Deref for I2cRepeaterGuard<'rtl, 'tx> {
    type Target = Transaction<'rtl>;

    fn deref(&self) -> &Self::Target {
        self.transaction
    }
}

impl<'rtl, 'tx> DerefMut for I2cRepeaterGuard<'rtl, 'tx> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.transaction
    }
}

impl<'rtl, 'tx> Drop for I2cRepeaterGuard<'rtl, 'tx> {
    fn drop(&mut self) {
        if self.transaction.i2c_repeater_enabled() {
            tracing::warn!("Dropped I2cRepeaterGuard without disabling I2C repeater");
        }
    }
}
