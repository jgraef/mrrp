use anyhow::{
    Error,
    anyhow,
};
use clap::Args;
use mrrp_rtl_sdr::{
    Device,
    DeviceInfo,
    rtl2832u::Rtl2832u,
};

#[derive(Debug, Args)]
pub struct DeviceArgs {
    /// Specify a device by serial number.
    ///
    /// If you only have one device connected, you can omit this.
    #[clap(short, long)]
    pub serial: Option<String>,

    /// Attempt to detach kernel modules before claiming the USB interface.
    #[clap(long)]
    pub detach_kernel: bool,
}

impl DeviceArgs {
    pub async fn find_device(&self) -> Result<DeviceInfo, Error> {
        for device_info in mrrp_rtl_sdr::enumerate_devices().await? {
            if self.serial.is_none() || device_info.serial_number() == self.serial.as_deref() {
                tracing::debug!(?device_info, "device found");
                return Ok(device_info);
            }
        }

        if let Some(serial) = self.serial.as_deref() {
            Err(anyhow!("Device not found: {serial}"))
        }
        else {
            Err(anyhow!("No device found"))
        }
    }

    pub async fn open_rtl2832u(&self) -> Result<(Rtl2832u, DeviceInfo), Error> {
        let device_info = self.find_device().await?;

        let rtl2832u = device_info.open_rtl2832u(false, Default::default()).await?;

        Ok((rtl2832u, device_info))
    }

    pub async fn open_device(&self) -> Result<Device, Error> {
        Ok(self.find_device().await?.open(Default::default()).await?)
    }
}
