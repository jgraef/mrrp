use std::time::Duration;

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

const INTERFACE: u8 = 0;

#[derive(Clone, Debug)]
pub struct OpenOptions {
    pub detach_kernel_driver: bool,
    pub reset_on_drop: bool,
    pub control_timeout: Duration,
    pub tuner_probe: AnyTunerProbe,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            detach_kernel_driver: false,
            reset_on_drop: true,
            control_timeout: Duration::from_secs(5),
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
    pub async fn open(device_info: DeviceInfo, options: OpenOptions) -> Result<Self, Error> {
        let usb_device = device_info.usb.open().await?;

        if options.detach_kernel_driver {
            usb_device.detach_kernel_driver(INTERFACE)?;
        }

        let usb_interface = usb_device.claim_interface(INTERFACE).await?;

        let mut rtl2832u = Rtl2832u::new(usb_interface, options.control_timeout);

        rtl2832u.initialize().await?;

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

        Ok(Self {
            device_info,
            reset_on_drop: options.reset_on_drop,
            inner: Some(Inner { rtl2832u, tuner }),
        })
    }

    pub async fn reset(mut self) -> Result<(), Error> {
        if let Some(mut inner) = self.inner.take() {
            inner.reset().await?;
        }

        Ok(())
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    // todo: only for testing
    pub fn rtl2832u(&mut self) -> &mut Rtl2832u {
        &mut self.inner.as_mut().expect("device lost").rtl2832u
    }
}

#[derive(Debug)]
struct Inner {
    rtl2832u: Rtl2832u,
    tuner: AnyTuner,
}

impl Inner {
    async fn reset(&mut self) -> Result<(), Error> {
        // todo: reset tuner

        self.rtl2832u.reset().await?;

        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
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
