//! From-scratch implementation of RTL-SDR driver library for async Rust.
//!
//! # Device enumeration
//!
//! The very first step towards using a RTL-SDR with this crate is to enumerate
//! the device that are available.
//!
//! ```
//! # async fn main_async() -> Result<(), Box<dyn std::error::Error>> {
//! use mrrp_rtl_sdr::enumerate_devices;
//!
//! for device_info in enumerate_devices().await? {
//!     println!("{device_info:?}");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Opening a device
//!
//! Once a suitable [`DeviceInfo`] is chosen, it can be opened:
//!
//! ```
//! # async fn main_async() -> Result<(), Box<dyn std::error::Error>> {
//! use mrrp_rtl_sdr::{
//!     OpenOptions,
//!     enumerate_devices,
//! };
//!
//! let device_info = enumerate_devices().await?.next().expect("no device found");
//!
//! let device = device_info.open(OpenOptions::default()).await?;
//! # Ok(())
//! # }
//! ```
//!
//! There's also a convenience function [`open_any`] that will open the first
//! device it finds:
//!
//! ```
//! # async fn main_async() -> Result<(), Box<dyn std::error::Error>> {
//! use mrrp_rtl_sdr::{
//!     OpenOptions,
//!     open_any,
//! };
//!
//! let device = open_any(OpenOptions::default()).await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Using the device
//!
//! Refer to the [`Device`] documentation for all methods that are supported.
//!
//! As a simple example we want to set the sample rate, center frequency and
//! then read samples.
//!
//! ```
//! # async fn main_async() -> Result<(), Box<dyn std::error::Error>> {
//! use mrrp_rtl_sdr::{open_any, OpenOptions};
//! use tokio::io::AsyncReadExt;
//!
//! let mut device = open_any(OpenOptions::default()).await?;
//!
//! // Set sample rate to 2.4 MSa/s
//! device.set_sample_rate(2_400_000.0).await?;
//!
//! // Set center frequency to 7 MHz
//! device.set_center_frequency(7_000_000.0).await?;
//!
//! // Start sampling and acquire a Reader
//! //
//! // By default the RTL2832U sends packets of 512 bytes. A large buffer gives your program more time until it needs to read the data (or packets get lost).
//! //
//! // The Reader can also be configured for multiple concurrent transfers. Refer to its documentation for more information.
//! let buffer_size = 64 * 1024;
//! let mut reader = device.reader(buffer_size).await?;
//!
//! // read samples. these are interleaved IQ as pairs of u8, with the equilibrium at 128.
//! loop {
//!     let i = reader.read_u8().await?;
//!     let q = reader.read_u8().await?;
//!     println!("i=0x{i:02x} q=0x{q:02x}");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Note that the [`Reader`] struct is indepedent of the [`Device`]. There's no
//! internal synchronization needed between reading and configuration, because
//! the data is handled by a completely separate USB endpoint.
//!
//! There is however, some synchronization, such that the [`Reader`] can
//! reconfigure the [`Device`] once it is stopped.
//!
//! # Proper shutdown
//!
//! Due to the lack of support for a async drop, you need to shutdown both
//! [`Device`] and [`Reader`] manually, if you want to ensure that the device or
//! data endpoint is reset properly.
//!
//! ```
//! # async fn main_async() -> Result<(), Box<dyn std::error::Error>> {
//! # let reader: mrrp_rtl_sdr::Reader = unreachable!();
//! # let device: mrrp_rtl_sdr::Device = unreachable!();
//!
//! // This will reconfigure the RTL2832U to reset the data endpoint.
//! reader.close().await?;
//!
//! // This will reset the whole device, including RTL2832U with data endpoint, and the tuner.
//! device.close().await?;
//! # Ok(())
//! # }
//! ```
//!
//! Closing the device will always reset the data endpoint, so any [`Reader`]
//! will also be closed.
//!
//! If you don't close [`Device`] or [`Reader`] manually, an attempt will be
//! made to do this in the background, when either is dropped. For this a tokio
//! task is spawned that performs the shutdown. This is not 100% reliable, if
//! e.g. the tokio runtime is shutdown before the task can run.
//!
//! # mrrp integration
//!
//! For [`Reader`] to implement `mrrp_core::AsyncReadSamples`, the `mrrp`
//! feature must be enabled. It is enabled by default, but can be disabled if
//! not needed.

pub mod device;
pub mod enumerate;
pub mod rtl2832u;
pub mod tuner;

use crate::tuner::AnyTunerError;
pub use crate::{
    device::{
        Device,
        Reader,
    },
    enumerate::{
        DeviceInfo,
        enumerate_devices,
    },
    tuner::gain::TunerGain,
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

    /// Detach the kernel driver before claiming the USB interface.
    ///
    /// This only works on Linux, and is ignored on other platforms.
    pub detach_kernel_driver: bool,
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

    #[allow(dead_code)]
    fn assert_unpin<T>()
    where
        T: Unpin,
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
    fn assert_reader_is_send_sync_unpin() {
        assert_send::<Reader>();
        assert_sync::<Reader>();
        assert_unpin::<Reader>();
    }
}
