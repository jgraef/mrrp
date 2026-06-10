//! GPIO pins
//!
//! You can obtain a [`GpioPin`] via [`gpio`](Rtl2832u::try_gpio).

use std::{
    ops::{
        Deref,
        DerefMut,
    },
    sync::{
        Arc,
        atomic::{
            AtomicU8,
            Ordering,
        },
    },
};

use bitfield::{
    Bit,
    BitMut,
    BitRange,
    BitRangeMut,
};

use crate::rtl2832u::{
    Error,
    Rtl2832u,
    register::sys as reg,
};

/// Direction of a GPIO pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Direction {
    /// GPIO input
    Input,
    /// GPIO output
    Output,
}

/// PAD configuration for a GPIO pin
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PadConfig {
    /// No pull-up or pull-down
    Normal,
    /// 75 kΩ pull-up resistor
    PullUp,
    /// 75 kΩ pull-down resistor
    PullDown,
    /// Invalid state
    Invalid,
}

impl PadConfig {
    fn from_bits(bits: u8) -> Self {
        match bits {
            0b00 => Self::Normal,
            0b01 => Self::PullDown,
            0b10 => Self::PullUp,
            0b11 => Self::Invalid,
            _ => panic!("Invalid bits provided: {bits:b}"),
        }
    }

    fn as_bits(&self) -> u8 {
        match self {
            Self::Normal => 0b00,
            Self::PullDown => 0b01,
            Self::PullUp => 0b10,
            Self::Invalid => 0b11,
        }
    }
}

impl Rtl2832u {
    /// Returns an unconfigured GPIO pin
    ///
    /// This will try to acquire exclusive access to a GPIO pin. It returns
    /// `Err(GpioPinBudy)`, if the pin is already in use.
    ///
    /// # Panic
    ///
    /// This panics if `pin >= 8`.
    pub fn try_gpio(&self, pin: u8) -> Result<GpioPin, GpioPinBusy> {
        assert!(pin < 8, "Invalid GPIO pin: {pin}");

        if self.gpio_state.try_lock_pin(pin) {
            Ok(GpioPin {
                gpio_state: self.gpio_state.clone(),
                pin,
            })
        }
        else {
            Err(GpioPinBusy { pin })
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct GpioState {
    pin_locks: AtomicU8,
}

impl GpioState {
    pub fn try_lock_pin(&self, pin: u8) -> bool {
        let pin_mask = 1 << pin;

        let pin_locks = self.pin_locks.fetch_or(pin_mask, Ordering::Relaxed);

        pin_locks & pin_mask == 0
    }

    pub fn unlock_pin(&self, pin: u8) {
        let pin_mask = !(1 << pin);
        let _was_locked = self.pin_locks.fetch_and(pin_mask, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("GPIO pin {pin} is already in use.")]
pub struct GpioPinBusy {
    pub pin: u8,
}

/// A unconfigured GPIO pin.
#[derive(Debug)]
pub struct GpioPin {
    gpio_state: Arc<GpioState>,
    pin: u8,
}

impl GpioPin {
    /// Returns the currently configured direction of the pin.
    pub async fn direction(&mut self, rtl2832u: &mut Rtl2832u) -> Result<Direction, Error> {
        let gpd = rtl2832u.read_register::<reg::GPD>().await?;

        let direction = if gpd.0.bit(self.pin.into()) {
            Direction::Input
        }
        else {
            Direction::Output
        };
        Ok(direction)
    }

    /// Returns the PAD configuration
    pub async fn pad_config(&mut self, rtl2832u: &mut Rtl2832u) -> Result<PadConfig, Error> {
        let gp_cfg = rtl2832u.read_register::<reg::GP_CFG>().await?;

        let bits: u8 = gp_cfg
            .0
            .bit_range((self.pin * 2 + 1).into(), (self.pin * 2).into());

        Ok(PadConfig::from_bits(bits))
    }

    /// Returns the PAD configuration
    pub async fn set_pad_config(
        &mut self,
        rtl2832u: &mut Rtl2832u,
        pad_config: PadConfig,
    ) -> Result<(), Error> {
        assert_ne!(
            pad_config,
            PadConfig::Invalid,
            "Invalid PAD config: {pad_config:?}"
        );

        tracing::debug!(pin = self.pin, ?pad_config, "set GPIO PAD config");

        rtl2832u
            .write_register_update::<reg::GP_CFG>(|gp_cfg| {
                gp_cfg.0.set_bit_range(
                    (self.pin * 2 + 1).into(),
                    (self.pin * 2).into(),
                    pad_config.as_bits(),
                );
            })
            .await?;
        Ok(())
    }

    /// Returns an input pin.
    ///
    /// This ensures the pin is configured for input.
    pub async fn into_input(self, rtl2832u: &mut Rtl2832u) -> Result<InputPin, Error> {
        tracing::debug!(pin = self.pin, "configuring GPIO pin for input");

        // configure pin as input
        rtl2832u
            .write_register_update::<reg::GPD>(|gpd| {
                gpd.0.set_bit(self.pin.into(), true);
            })
            .await?;

        Ok(InputPin { pin: self })
    }

    /// Returns an output pin with an initial state.
    ///
    /// This ensures the pin is configured for output, and the pin state is
    /// initialized before it's enabled.
    pub async fn into_output_init(
        self,
        rtl2832u: &mut Rtl2832u,
        initial_state: bool,
    ) -> Result<OutputPin, Error> {
        let pin = self.pin;

        tracing::debug!(?pin, ?initial_state, "configuring GPIO pin for output");

        let mut pin = self
            .into_output_inner(rtl2832u, async |rtl2832u: &mut Rtl2832u| {
                // set initial state
                rtl2832u
                    .write_register_update::<reg::GPO>(|gpo| {
                        gpo.0.set_bit(pin.into(), initial_state);
                    })
                    .await
            })
            .await?;

        pin.cached_state = Some(initial_state);

        Ok(pin)
    }

    /// Returns an output pin with an initial state.
    ///
    /// This ensures the pin is configured for output.
    pub async fn into_output(self, rtl2832u: &mut Rtl2832u) -> Result<OutputPin, Error> {
        tracing::debug!(pin = ?self.pin, "configuring GPIO pin for output");

        self.into_output_inner(rtl2832u, async |_| Ok(())).await
    }

    async fn into_output_inner(
        self,
        rtl2832u: &mut Rtl2832u,
        pre_enable_hook: impl AsyncFnOnce(&mut Rtl2832u) -> Result<(), Error>,
    ) -> Result<OutputPin, Error> {
        // todo: do we have to disable the output first, so that it isn't in an invalid
        // state once we configure it as output?

        // configure pin as output

        rtl2832u
            .write_register_update::<reg::GPD>(|gpd| {
                gpd.0.set_bit(self.pin.into(), false);
            })
            .await?;

        pre_enable_hook(&mut *rtl2832u).await?;

        // enable output
        rtl2832u
            .write_register_update::<reg::GPOE>(|gpoe| {
                gpoe.0.set_bit(self.pin.into(), true);
            })
            .await?;

        Ok(OutputPin {
            pin: self,
            cached_state: None,
        })
    }
}

impl Drop for GpioPin {
    fn drop(&mut self) {
        self.gpio_state.unlock_pin(self.pin)
    }
}

/// A GPIO pin configured for input.
#[derive(Debug)]
pub struct InputPin {
    pin: GpioPin,
}

impl<'a> InputPin {
    /// Read the logic level at the pin.
    pub async fn read(&mut self, rtl2832u: &mut Rtl2832u) -> Result<bool, Error> {
        let gpi = rtl2832u.read_register::<reg::GPI>().await?;
        let state = gpi.0.bit(self.pin.pin.into());

        tracing::debug!(pin = ?self.pin.pin, ?state, "read GPIO pin");

        Ok(state)
    }
}

impl<'a> Deref for InputPin {
    type Target = GpioPin;

    fn deref(&self) -> &Self::Target {
        &self.pin
    }
}

impl<'a> DerefMut for InputPin {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pin
    }
}

/// A GPIO pin configured for output.
#[derive(Debug)]
pub struct OutputPin {
    pin: GpioPin,

    /// Cache the currently known state of the pin.
    ///
    /// This is technically also cached in the shadow map in [`Rtl2832u`], but
    /// we would need to get a lock for this (i.e. a transaction) to read it.
    /// And since we have exclusive access to this pin, we can cache it here.
    cached_state: Option<bool>,
}

impl OutputPin {
    /// Read the current output logic level of this pin.
    pub async fn get_state(&mut self, rtl2832u: &mut Rtl2832u) -> Result<bool, Error> {
        if let Some(cached_state) = self.cached_state {
            Ok(cached_state)
        }
        else {
            let gpo = rtl2832u.read_register::<reg::GPO>().await?;

            let state = gpo.0.bit(self.pin.pin.into());

            self.cached_state = Some(state);

            Ok(state)
        }
    }

    /// Set the output logic level for this pin.
    pub async fn write(&mut self, rtl2832u: &mut Rtl2832u, state: bool) -> Result<(), Error> {
        tracing::debug!(pin = ?self.pin.pin, cached_state = ?self.cached_state, ?state, "writing GPIO pin");

        if self
            .cached_state
            .is_none_or(|cached_state| cached_state != state)
        {
            rtl2832u
                .write_register_update::<reg::GPO>(|gpo| {
                    gpo.0.set_bit(self.pin.pin.into(), state);
                })
                .await?;

            self.cached_state = Some(state);
        }

        Ok(())
    }

    /// Returns the state without reading from the device, if it is known.
    ///
    /// This can be done without even acquiring a transaction on the
    /// [`Rtl2832u`], so it doesn't an async context, and can't fail with an
    /// errorr.
    ///
    /// But this can only return a state if it was previously set, read via
    /// [`get_state`](Self::get_state), or initialized via
    /// [`into_output_init`](GpioPin::into_output_init).
    #[inline(always)]
    pub fn get_known_state(&self) -> Option<bool> {
        self.cached_state
    }
}

impl Deref for OutputPin {
    type Target = GpioPin;

    fn deref(&self) -> &Self::Target {
        &self.pin
    }
}

impl<'a> DerefMut for OutputPin {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pin
    }
}
