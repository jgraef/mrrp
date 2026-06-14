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
        IfSetting,
        Tuner,
        TunerError,
        TunerProbe,
        r82xx::{
            self,
            R82xx,
            R82xxProbe,
            RfInput,
            TrackingFilterSetting,
            preset::is_in_notch_band,
        },
    },
};

/// GPIO pin that switches the upconverter.
pub const UPCONVERTER_GPIO_PIN: u8 = 5;

/// The logic state of the GPIO pin at which the upconverter is enabled.
pub const UPCONVERTER_GPIO_ENABLE: bool = false;

pub const BLOG_CRYSTAL_FREQ: u32 = rtl2832u::DEFAULT_CRYSTAL_FREQUENCY;

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
        if let Some(mut r82xx) = R82xxProbe.try_open(rtl2832u).await?
            && r82xx.model() == r82xx::Model::R828D
        {
            r82xx.crystal_frequency = BLOG_CRYSTAL_FREQ as f32;

            let mut upconverter_pin = rtl2832u
                .try_gpio(UPCONVERTER_GPIO_PIN)?
                .into_output_init(rtl2832u, !UPCONVERTER_GPIO_ENABLE)
                .await?;
            let pad_config = upconverter_pin.pad_config(rtl2832u).await?;
            tracing::debug!(?pad_config, "upconverter PAD config");

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
}

impl BlogTuner {
    pub fn new(r82xx: R82xx, model: Model, upconverter_pin: OutputPin) -> Self {
        let name = format!("blog-{model:?}-{}", r82xx.name());

        Self {
            r82xx,
            model,
            name,
            upconverter_pin,
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
        // which band (HF, VHF, UHF) are we tuning to?
        let band = Band::from_frequency(center_frequency);

        // does this band use the upconverter?
        let use_upconverter = band == Band::Hf;

        // which RF input should we be using?
        let rf_input = match (band, self.model) {
            (Band::Hf, Model::V4) => RfInput::Cable2,
            (Band::Hf, Model::V4l) => RfInput::Cable1,
            (Band::Vhf, Model::V4) => RfInput::Cable1,
            _ => RfInput::Air,
        };

        let is_in_notch_band = is_in_notch_band(center_frequency);

        tracing::debug!(
            ?center_frequency,
            ?band,
            ?use_upconverter,
            ?rf_input,
            ?is_in_notch_band,
            "setting center frequency"
        );

        // toggle upconverter on or off
        //
        // note: since `UPCONVERTER_GPIO_ENABLE` is `false`, the boolean expression can
        // be shortened to `!use_upconverter`.
        self.upconverter_pin
            .write(rtl2832u, use_upconverter ^ !UPCONVERTER_GPIO_ENABLE)
            .await?;

        // if we're using the upconverter we need to adjust the center frequency that we
        // tune the R82xx to
        if use_upconverter {
            center_frequency += 28800000.0;
        }

        let mut transaction = self.r82xx.begin_transaction(rtl2832u);

        // select the RF input with the R82xx
        // this sets cable1_in, cable2_in and pwd_lna1/air_in
        transaction.select_rf_input(rf_input);

        // set frequency in R82xx
        //
        // todo: we should discard the transaction explicitely, if this fails.
        transaction.set_center_frequency(center_frequency).await?;

        // if upconverter is used, disable tracking filter
        if use_upconverter {
            transaction.set_tracking_filter_setting(&TrackingFilterSetting::bypass());
        }

        // disable notch filters when in notch band
        transaction.registers.set_open_d(!is_in_notch_band);

        /*
        // r82xx-reg-dump /home/emma/code/mrrp/tmp/rtl-sdr-blog/src/tuner_r82xx.c 1346
        // e3 30 75 c0 40 c5 8f 68 53 75 68 64 bb 06 31 c6 10 81 28 48 ec 2a 14 24 dd 6e
        // 40
        let expected = [
            0xe3, 0x30, 0x75, 0xc0, 0x40, 0xc5, 0x8f, 0x68, 0x53, 0x75, 0x68, 0x64, 0xbb, 0x06,
            0x31, 0xc6, 0x10, 0x81, 0x28, 0x48, 0xec, 0x2a, 0x14, 0x24, 0xdd, 0x6e, 0x40,
        ];

        for i in 0x05..0x20 {
            if transaction.registers[i] != expected[usize::from(i - 0x05)] {
                println!(
                    "Register 0x{i:02x} differs:\n  expected: 0x{:02x}\n  provided: 0x{:02x}",
                    expected[usize::from(i - 0x05)],
                    transaction.registers[i]
                );
            }
        }
         */

        transaction.commit().await?;

        Ok(())
    }

    fn if_setting(&self) -> IfSetting {
        self.r82xx.if_setting()
    }

    async fn shutdown<'a>(&'a mut self, rtl2832u: &'a mut Rtl2832u) -> Result<(), Self::Error> {
        self.upconverter_pin
            .write(rtl2832u, !UPCONVERTER_GPIO_ENABLE)
            .await?;

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
        if frequency <= 28_800_000.0 {
            Self::Hf
        }
        else if frequency <= 250_000_000.0 {
            Self::Vhf
        }
        else {
            Self::Uhf
        }
    }
}
