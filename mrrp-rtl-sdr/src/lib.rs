mod device;
pub mod enumerate;
pub mod rtl2832u;
pub mod tuner;

pub use enumerate::enumerate_devices;

pub use crate::device::{
    Device,
    OpenOptions,
};
use crate::tuner::{
    AnyTunerError,
    TunerError,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Usb(#[from] nusb::Error),

    #[error(transparent)]
    Rtl2832u(#[from] rtl2832u::Error),

    #[error("No device found")]
    NoDeviceFound,

    #[error(transparent)]
    Tuner(#[from] AnyTunerError),

    #[error("No tuner detected")]
    NoTunerFound,
}

pub async fn open_any(options: OpenOptions) -> Result<Device, Error> {
    enumerate_devices()
        .await?
        .next()
        .ok_or(Error::NoDeviceFound)?
        .open(options)
        .await
}
