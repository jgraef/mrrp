use crate::{
    Error,
    enumerate::DeviceInfo,
    rtl2832u::Rtl2832u,
    tuner::{
        AnyTuner,
        AnyTunerProbe,
        TunerProbe,
    },
};

#[derive(Clone, Debug)]
pub struct Options {
    pub reset_on_drop: bool,
    pub tuner_probe: AnyTunerProbe,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            reset_on_drop: true,
            tuner_probe: AnyTunerProbe::default(),
        }
    }
}

#[derive(Debug)]
pub struct Device {
    device_info: DeviceInfo,
    reset_on_drop: bool,

    /// The RTL2832U and tuner are in an `Option` so we can take them out and
    /// spawn a task to run the reset code if this struct is dropped.
    inner: Option<Inner>,
}

impl Device {
    pub async fn from_rtl2832u(
        mut rtl2832u: Rtl2832u,
        device_info: DeviceInfo,
        options: Options,
    ) -> Result<Self, Error> {
        // todo: should we try to reset the device if initialization fails?

        // initialize baseband
        rtl2832u.initialize().await?;

        // probe tuners
        let tuner = rtl2832u
            .with_i2c_repeater::<_, Error>(async |mut rtl2832u| {
                options
                    .tuner_probe
                    .try_open(&mut rtl2832u)
                    .await
                    .map_err(Into::into)
            })
            .await?
            .ok_or(Error::NoTunerFound)?;

        // if blog v4, set tuner_xtal = R828D_XTAL_FREQ, otherwise use rtl_xtal
        //
        // #define R828D_XTAL_FREQ		16000000
        // #define DEF_RTL_XTAL_FREQ	28800000
        // #define MIN_RTL_XTAL_FREQ	(DEF_RTL_XTAL_FREQ - 1000)
        // #define MAX_RTL_XTAL_FREQ	(DEF_RTL_XTAL_FREQ + 1000)

        Ok(Self {
            device_info,
            reset_on_drop: options.reset_on_drop,
            inner: Some(Inner { rtl2832u, tuner }),
        })
    }

    pub async fn close(mut self) -> Result<(), Error> {
        if let Some(mut inner) = self.inner.take() {
            inner.reset().await?;
        }

        Ok(())
    }

    #[inline(always)]
    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    #[inline(always)]
    fn expect_inner_mut(&mut self) -> &mut Inner {
        self.inner.as_mut().expect("device lost")
    }

    // for testing only
    pub fn tuner(&mut self) -> &mut AnyTuner {
        &mut self.expect_inner_mut().tuner
    }
}

#[derive(Debug)]
struct Inner {
    rtl2832u: Rtl2832u,
    tuner: AnyTuner,
}

impl Inner {
    async fn reset(&mut self) -> Result<(), Error> {
        // todo: reset gpio

        self.rtl2832u.reset().await?;

        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        tracing::debug!(reset_on_drop = ?self.reset_on_drop, inner_present = self.inner.is_some(), "device dropped");

        if self.reset_on_drop
            && let Some(mut inner) = self.inner.take()
        {
            tokio::spawn(async move {
                if let Err(error) = inner.reset().await {
                    tracing::error!(%error, "Error resetting RTL2832U while dropping");
                }
            });
        }
    }
}
