//! GPIO pins
//!
//! You can obtain a [`GpioPin`] via [`gpio`](Rtl2832u::gpio).

use std::ops::{
    Deref,
    DerefMut,
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
    pub fn gpio(&mut self, pin: u8) -> GpioPin<'_> {
        assert!(pin < 8, "Invalid GPIO pin: {pin}");
        GpioPin {
            rtl2832u: self,
            pin,
        }
    }
}

/// A unconfigured GPIO pin.
#[derive(Debug)]
pub struct GpioPin<'a> {
    rtl2832u: &'a mut Rtl2832u,
    pin: u8,
}

impl<'a> GpioPin<'a> {
    /// Returns the currently configured direction of the pin.
    pub async fn direction(&mut self) -> Result<Direction, Error> {
        let gpd = self.rtl2832u.read_register::<reg::GPD>().await?;

        let direction = if gpd.0.bit(self.pin.into()) {
            Direction::Input
        }
        else {
            Direction::Output
        };
        Ok(direction)
    }

    /// Returns the PAD configuration
    pub async fn pad_config(&mut self) -> Result<PadConfig, Error> {
        let gp_cfg = self.rtl2832u.read_register::<reg::GP_CFG>().await?;

        let bits: u8 = gp_cfg
            .0
            .bit_range((self.pin * 2 + 1).into(), (self.pin * 2).into());

        Ok(PadConfig::from_bits(bits))
    }

    /// Returns the PAD configuration
    pub async fn set_pad_config(&mut self, pad_config: PadConfig) -> Result<(), Error> {
        assert_ne!(
            pad_config,
            PadConfig::Invalid,
            "Invalid PAD config: {pad_config:?}"
        );

        self.rtl2832u
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
    pub async fn into_input(self) -> Result<InputPin<'a>, Error> {
        // configure pin as input
        self.rtl2832u
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
    pub async fn gp_output_init(self, initial_state: bool) -> Result<OutputPin<'a>, Error> {
        let pin = self.pin;
        self.into_output_inner(async |rtl2832u| {
            // set initial state
            rtl2832u
                .write_register_update::<reg::GPO>(|gpo| {
                    gpo.0.set_bit(pin.into(), initial_state);
                })
                .await
        })
        .await
    }

    /// Returns an output pin with an initial state.
    ///
    /// This ensures the pin is configured for output.
    pub async fn into_output(self) -> Result<OutputPin<'a>, Error> {
        self.into_output_inner(async |_| Ok(())).await
    }

    async fn into_output_inner(
        mut self,
        pre_enable_hook: impl AsyncFnOnce(&mut Rtl2832u) -> Result<(), Error>,
    ) -> Result<OutputPin<'a>, Error> {
        // todo: do we have to disable the output first, so that it isn't in an invalid
        // state once we configure it as output?

        // configure pin as output
        self.rtl2832u
            .write_register_update::<reg::GPD>(|gpd| {
                gpd.0.set_bit(self.pin.into(), false);
            })
            .await?;

        pre_enable_hook(&mut self.rtl2832u).await?;

        // enable output
        self.rtl2832u
            .write_register_update::<reg::GPOE>(|gpoe| {
                gpoe.0.set_bit(self.pin.into(), true);
            })
            .await?;

        Ok(OutputPin { pin: self })
    }
}

/// A GPIO pin configured for input.
#[derive(Debug)]
pub struct InputPin<'a> {
    pin: GpioPin<'a>,
}

impl<'a> InputPin<'a> {
    /// Read the logic level at the pin.
    pub async fn read(&mut self) -> Result<bool, Error> {
        let gpi = self.pin.rtl2832u.read_register::<reg::GPI>().await?;
        Ok(gpi.0.bit(self.pin.pin.into()))
    }
}

impl<'a> Deref for InputPin<'a> {
    type Target = GpioPin<'a>;

    fn deref(&self) -> &Self::Target {
        &self.pin
    }
}

impl<'a> DerefMut for InputPin<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pin
    }
}

/// A GPIO pin configured for output.
#[derive(Debug)]
pub struct OutputPin<'a> {
    pin: GpioPin<'a>,
}

impl<'a> OutputPin<'a> {
    /// Read the current output logic level of this pin.
    pub async fn get_state(&mut self) -> Result<bool, Error> {
        let gpo = self.pin.rtl2832u.read_register::<reg::GPO>().await?;
        Ok(gpo.0.bit(self.pin.pin.into()))
    }

    /// Set the output logic level for this pin.
    pub async fn write(&mut self, state: bool) -> Result<(), Error> {
        self.pin
            .rtl2832u
            .write_register_update::<reg::GPO>(|gpo| {
                gpo.0.set_bit(self.pin.pin.into(), state);
            })
            .await?;
        Ok(())
    }
}

impl<'a> Deref for OutputPin<'a> {
    type Target = GpioPin<'a>;

    fn deref(&self) -> &Self::Target {
        &self.pin
    }
}

impl<'a> DerefMut for OutputPin<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pin
    }
}
