//! GPIO pins
//!
//! See [`gp_direction`](Rtl2832u::gp_direction),
//! [`gp_input`](Rtl2832u::gp_input),
//! [`gp_output_init`](Rtl2832u::gp_output_init), and
//! [`gp_output`](Rtl2832u::gp_output)

use bitfield::{
    Bit,
    BitMut,
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

impl Rtl2832u {
    /// Returns the currently configured direction of the pin.
    pub async fn gp_direction(&mut self, pin: u8) -> Result<Direction, Error> {
        assert!(pin < 8, "Invalid GPIO pin: {pin}");

        let gpd = self.read_register::<reg::GPD>().await?;

        let direction = if gpd.0.bit(pin.into()) {
            Direction::Input
        }
        else {
            Direction::Output
        };
        Ok(direction)
    }

    /// Returns an input pin.
    ///
    /// This ensures the pin is configured for input.
    pub async fn gp_input(&mut self, pin: u8) -> Result<InputPin<'_>, Error> {
        assert!(pin < 8, "Invalid GPIO pin: {pin}");

        // note: if we merge these 4 registers into 1 32bit registers we could do this
        // in one read/write

        // configure pin as input
        self.write_register_update::<reg::GPD>(|gpd| {
            gpd.0.set_bit(pin.into(), true);
        })
        .await?;

        Ok(InputPin {
            rtl2832u: self,
            pin,
        })
    }

    /// Returns an output pin with an initial state.
    ///
    /// This ensures the pin is configured for output, and the pin state is
    /// initialized before it's enabled.
    pub async fn gp_output_init(
        &mut self,
        pin: u8,
        initial_state: bool,
    ) -> Result<OutputPin<'_>, Error> {
        self.gp_output_inner(pin, async |rtl2832u| {
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
    pub async fn gp_output(&mut self, pin: u8) -> Result<OutputPin<'_>, Error> {
        self.gp_output_inner(pin, async |_| Ok(())).await
    }

    async fn gp_output_inner(
        &mut self,
        pin: u8,
        pre_enable_hook: impl AsyncFnOnce(&mut Rtl2832u) -> Result<(), Error>,
    ) -> Result<OutputPin<'_>, Error> {
        assert!(pin < 8, "Invalid GPIO pin: {pin}");

        // todo: do we have to disable the output first, so that it isn't in an invalid
        // state once we configure it as output?

        // configure pin as output
        self.write_register_update::<reg::GPD>(|gpd| {
            gpd.0.set_bit(pin.into(), false);
        })
        .await?;

        pre_enable_hook(self).await?;

        // enable output
        self.write_register_update::<reg::GPOE>(|gpoe| {
            gpoe.0.set_bit(pin.into(), true);
        })
        .await?;

        Ok(OutputPin {
            rtl2832u: self,
            pin,
        })
    }
}

/// A GPIO pin configured for input.
#[derive(Debug)]
pub struct InputPin<'a> {
    rtl2832u: &'a mut Rtl2832u,
    pin: u8,
}

impl<'a> InputPin<'a> {
    /// Read the logic level at the pin.
    pub async fn read(&mut self) -> Result<bool, Error> {
        let gpi = self.rtl2832u.read_register::<reg::GPI>().await?;
        Ok(gpi.0.bit(self.pin.into()))
    }
}

/// A GPIO pin configured for output.
#[derive(Debug)]
pub struct OutputPin<'a> {
    rtl2832u: &'a mut Rtl2832u,
    pin: u8,
}

impl<'a> OutputPin<'a> {
    /// Read the current output logic level of this pin.
    pub async fn get_state(&mut self) -> Result<bool, Error> {
        let gpo = self.rtl2832u.read_register::<reg::GPO>().await?;
        Ok(gpo.0.bit(self.pin.into()))
    }

    /// Set the output logic level for this pin.
    pub async fn write(&mut self, state: bool) -> Result<(), Error> {
        self.rtl2832u
            .write_register_update::<reg::GPO>(|gpo| {
                gpo.0.set_bit(self.pin.into(), state);
            })
            .await?;
        Ok(())
    }
}
