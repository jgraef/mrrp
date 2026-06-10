//! Wrapper for [`R82xx`] with a RTLSDR Blog V4(L)
//!
//! This wrapper will handle the upconverter and configure the R828D
//! specifically for a Blog V4(L).
//!
//! # RF paths
//!
//! On both V4 and V4L the UHF path is air_in. Afaik air_in is selected if
//! cable_1 and cable_2 are disabled, but pwd_lna1 also plays a role. pwd_lna1
//! is only active if air_in is selected in librtlsdr, but we're not sure if
//! this actually switches an RF path on the R828D.
//!
//! If the frequency is below 28.8 MHz (i.e. HF) the upconverter is enabled
//! (GPIO pin 4 low), which moves the input signal up by 28.8 MHz. On the V4 the
//! upconverter is connected to cable_2, on the V4L to cable_1.
//!
//! On the V4 (not V4L) cable_1 is used for VHF (28.8 to 250 MHz), but it's
//! unclear why it's switched.
//!
//! On the V4L the only other input besides cable_1 (HF) is air_in, so that is
//! used for VHF and UHF.
//!
//! # GPIO
//!
//! Both V4 and V4L use GPIO pin 4 to switch the upconverter. It's active if the
//! pin is low.
//!
//! By default this pin is configured:
//!
//! - direction: output
//! - output enable: true
//! - state: low
//! - pad config: pull-down
//!
//! # TODO
//!
//! Move this config into a struct and make a generic Upconverter tuner
//! wrapper.

use crate::{
    rtl2832u::{
        self,
        Rtl2832u,
        gpio::{
            GpioPinBusy,
            OutputPin,
        },
    },
    tuner::{
        Tuner,
        TunerError,
        TunerProbe,
        r82xx::{
            self,
            R82xx,
            R82xxProbe,
            RfInput,
        },
    },
};

/// GPIO pin that switches the upconverter.
pub const UPCONVERTER_GPIO_PIN: u8 = 5;

/// The logic state of the GPIO pin at which the upconverter is enabled.
pub const UPCONVERTER_GPIO_ENABLE: bool = false;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    R82xx(#[from] r82xx::Error),

    #[error(transparent)]
    GpioBusy(#[from] GpioPinBusy),

    #[error(transparent)]
    Rtl2823u(#[from] rtl2832u::Error),
}

impl TunerError for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Model {
    V4,
    V4l,
}

impl Model {
    pub fn try_from_device_info(device_info: &nusb::DeviceInfo) -> Option<Self> {
        match (
            device_info.manufacturer_string(),
            device_info.product_string(),
        ) {
            (Some("RTLSDRBlog"), Some("Blog V4")) => Some(Model::V4),
            (Some("RTLSDRBlog"), Some("Blog V4L")) => Some(Model::V4l),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlogTunerProbe {
    model: Model,
}

impl BlogTunerProbe {
    pub fn from_model(model: Model) -> Self {
        Self { model }
    }

    pub fn try_from_device_info(device_info: &nusb::DeviceInfo) -> Option<Self> {
        Model::try_from_device_info(device_info).map(Self::from_model)
    }
}

impl TunerProbe for BlogTunerProbe {
    type Error = Error;
    type Tuner = BlogTuner;

    async fn try_open<'a>(
        &'a self,
        rtl2832u: &'a mut Rtl2832u,
    ) -> Result<Option<Self::Tuner>, Self::Error> {
        if let Some(r82xx) = R82xxProbe.try_open(rtl2832u).await?
            && r82xx.model() == r82xx::Model::R828D
        {
            // fixme: we have a transaction open
            let upconverter_pin = rtl2832u
                .try_gpio(UPCONVERTER_GPIO_PIN)?
                .into_output_init(rtl2832u, false)
                .await?;

            Ok(Some(BlogTuner::new(r82xx, self.model, upconverter_pin)))
        }
        else {
            Ok(None)
        }
    }
}

/// Wraps [`R82xx`] to add support for the RTL-SDR Blog V4's upconverter.
#[derive(Debug)]
pub struct BlogTuner {
    r82xx: R82xx,
    model: Model,
    name: String,
    upconverter_pin: OutputPin,

    selected_band: Option<Band>,
}

impl BlogTuner {
    pub fn new(r82xx: R82xx, model: Model, upconverter_pin: OutputPin) -> Self {
        let name = format!("blog-{model:?}-{}", r82xx.name());

        Self {
            r82xx,
            model,
            name,
            upconverter_pin,
            selected_band: None,
        }
    }
}

impl BlogTuner {
    pub fn upconverter_state(&self) -> bool {
        // we should always know the state since we initialized the pin
        self.upconverter_pin
            .get_known_state()
            .expect("state was initialized")
    }
}

impl Tuner for BlogTuner {
    type Error = Error;

    fn name(&self) -> &str {
        &self.name
    }

    async fn set_bandwidth<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        bandwidth: f32,
    ) -> Result<(), Self::Error> {
        self.r82xx.set_bandwidth(rtl2832u, bandwidth).await?;
        Ok(())
    }

    async fn set_center_frequency<'a>(
        &'a mut self,
        rtl2832u: &'a mut Rtl2832u,
        mut center_frequency: f32,
    ) -> Result<(), Self::Error> {
        // todo: transactions for the r82xx, so we only flush once we're done here. this
        // would also make it easy to check which registers actually changed

        let mut transaction = self.r82xx.begin_transaction(rtl2832u);

        // which band (HF, VHF, UHF) are we tuning to?
        let band = Band::from_frequency(center_frequency);

        // does this band use the upconverter?
        let use_upconverter = band == Band::Hf;

        tracing::debug!(
            ?center_frequency,
            ?band,
            ?use_upconverter,
            "setting center frequency"
        );

        // check if we are switching to a different band
        if self
            .selected_band
            .is_none_or(|selected_band| selected_band != band)
        {
            // switch band

            // toggle upconverter on or off
            //self.upconverter_pin
            //.write(use_upconverter ^ UPCONVERTER_GPIO_ENABLE)
            //.await?;

            // which RF input should we be using?
            let rf_input = match (band, self.model) {
                (Band::Hf, Model::V4) => RfInput::Cable2,
                (Band::Hf, Model::V4l) => RfInput::Cable1,
                (Band::Vhf, Model::V4) => RfInput::Cable1,
                _ => RfInput::Air,
            };

            tracing::debug!(from = ?self.selected_band, to = ?band, ?rf_input, "switching band");

            // select the RF input with the R82xx
            transaction.select_rf_input(rf_input);
        }

        // if we're using the upconverter we need to adjust the center frequency that we
        // tune the R82xx to
        if use_upconverter {
            center_frequency += 28800000.0;
        }

        // set frequency in R82xx
        transaction.set_center_frequency(center_frequency);

        transaction.commit().await?;

        // store current band
        //
        // note: we can only do this now, after we know that the transaction didn't
        // fail.
        self.selected_band = Some(band);

        Ok(())
    }

    async fn shutdown<'a>(&'a mut self, rtl2832u: &'a mut Rtl2832u) -> Result<(), Self::Error> {
        self.r82xx.shutdown(rtl2832u).await?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Band {
    Hf,
    Vhf,
    Uhf,
}

impl Band {
    pub fn from_frequency(frequency: f32) -> Self {
        if frequency <= 28800000.0 {
            Self::Hf
        }
        else if frequency <= 250000000.0 {
            Self::Vhf
        }
        else {
            Self::Uhf
        }
    }
}
