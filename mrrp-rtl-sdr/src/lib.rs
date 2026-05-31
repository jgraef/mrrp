pub mod enumerate;
pub mod i2c;
pub mod rtl283u;

use std::time::Duration;

pub use enumerate::enumerate_devices;

use crate::{
    enumerate::DeviceInfo,
    rtl283u::Rtl283u,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Usb(#[from] nusb::Error),

    #[error(transparent)]
    UsbTransfer(#[from] nusb::transfer::TransferError),

    #[error("No device found")]
    NoDeviceFound,

    #[error(
        "Invalid control response: expected {expected_length} bytes, but received {response_length} bytes."
    )]
    InvalidControlResponse {
        expected_length: usize,
        response_length: usize,
    },
}

pub async fn open_first(options: OpenOptions) -> Result<Device, Error> {
    enumerate_devices()
        .await?
        .next()
        .ok_or(Error::NoDeviceFound)?
        .open(options)
        .await
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    pub detach_kernel_driver: bool,
    pub control_timeout: Duration,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            detach_kernel_driver: false,
            control_timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Debug)]
pub struct Device {
    device_info: DeviceInfo,
    rtl283u: Rtl283u,
}

impl Device {
    pub async fn open(device_info: DeviceInfo, options: OpenOptions) -> Result<Self, Error> {
        let usb_device = device_info.usb.open().await?;

        if options.detach_kernel_driver {
            usb_device.detach_kernel_driver(INTERFACE)?;
        }

        let usb_interface = usb_device.claim_interface(INTERFACE).await?;

        for interface_descriptor in usb_interface.descriptors() {
            tracing::debug!("{interface_descriptor:#?}");
        }

        let mut rtl283u = Rtl283u::new(usb_interface, options.control_timeout);

        //rtl283u.initialize_baseband().await?;
        rtl283u.test().await?;

        Ok(Self {
            device_info,
            rtl283u,
        })
    }

    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }
}

const INTERFACE: u8 = 0;
