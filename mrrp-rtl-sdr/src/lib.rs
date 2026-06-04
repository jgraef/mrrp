//! From-scratch implementation of RTL-SDR driver library for async Rust.

mod device;
pub mod enumerate;
pub mod rtl2832u;
pub mod tuner;

use crate::tuner::AnyTunerError;
pub use crate::{
    device::Device,
    enumerate::{
        DeviceInfo,
        enumerate_devices,
    },
    rtl2832u::Reader,
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

#[derive(Clone, Debug, Default)]
pub struct OpenOptions {
    pub device: device::Options,
    pub rtl2832u: rtl2832u::Options,
}

mod assert_send_sync {
    #![allow(dead_code)]

    use crate::{
        Device,
        Reader,
        rtl2832u::Rtl2832u,
    };

    fn assert_send<T>()
    where
        T: Send,
    {
    }

    #[allow(dead_code)]
    fn assert_sync<T>()
    where
        T: Sync,
    {
    }

    /// this makes sure that [`Device`] is `Send + Sync`
    fn assert_device_is_send_sync() {
        assert_send::<Device>();
        assert_sync::<Device>();
    }

    /// this makes sure that [`Rtl2832u`] is `Send + Sync`
    fn assert_rtl2832u_is_send_sync() {
        assert_send::<Rtl2832u>();
        assert_sync::<Rtl2832u>();
    }

    /// this makes sure that [`Reader`] is `Send + Sync`
    fn assert_reader_is_send_sync() {
        assert_send::<Reader>();
        assert_sync::<Reader>();
    }
}
