//! Enumerate RTL2832U devices via USB.

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::OnceLock,
};

use crate::{
    Device,
    Error,
    OpenOptions,
    rtl2832u::{
        self,
        Rtl2832u,
    },
};

/// Enumerate RTL2832U devices via USB.
///
/// Returns an iterator over all detected devices, yielding [`DeviceInfo`].
pub async fn enumerate_devices() -> Result<EnumerateDevices, Error> {
    Ok(EnumerateDevices {
        devices: Box::new(nusb::list_devices().await?),
        known_devices: BuiltinKnownDevices::builtin(),
    })
}

/// Iterator over detected devices.
///
/// Yields [`DeviceInfo`].
#[derive(derive_more::Debug)]
pub struct EnumerateDevices<K = &'static BuiltinKnownDevices> {
    #[debug(skip)]
    devices: Box<dyn Iterator<Item = nusb::DeviceInfo>>,

    known_devices: K,
}

impl<K> EnumerateDevices<K> {
    /// Use a different method of detecting known devices.
    ///
    /// The argument `known_devices` must implement [`KnownDevices`] for
    /// [`EnumerateDevices`] to still work.
    pub fn with_known_devices<K2>(self, known_devices: K2) -> EnumerateDevices<K2> {
        EnumerateDevices {
            devices: self.devices,
            known_devices,
        }
    }
}

impl<K> Iterator for EnumerateDevices<K>
where
    K: KnownDevices,
{
    type Item = DeviceInfo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let device_info = self.devices.next()?;

            if let Some(known_device) = self.known_devices.detect(&device_info) {
                return Some(DeviceInfo {
                    usb: device_info,
                    device_config: known_device,
                });
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.devices.size_hint()
    }
}

/// A discovered RTL8232U device.
///
/// This can be used to [`open`](Self::open) the device.
#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub(crate) usb: nusb::DeviceInfo,
    pub(crate) device_config: DeviceConfig,
}

impl DeviceInfo {
    pub fn vendor_id(&self) -> u16 {
        self.usb.vendor_id()
    }

    pub fn product_id(&self) -> u16 {
        self.usb.product_id()
    }

    pub fn manufacturer_string(&self) -> Option<&str> {
        self.usb.manufacturer_string()
    }

    pub fn product_string(&self) -> Option<&str> {
        self.usb.product_string()
    }

    pub fn serial_number(&self) -> Option<&str> {
        self.usb.serial_number()
    }

    pub fn device_config(&self) -> &DeviceConfig {
        &self.device_config
    }

    /// Open the device
    pub async fn open(self, options: OpenOptions) -> Result<Device, Error> {
        let rtl2832u = self.open_rtl2832u(options.rtl2832u).await?;
        Device::from_rtl2832u(rtl2832u, self, options.device).await
    }

    /// Open a low-level [`Rtl2832u`] interface to the device.
    pub async fn open_rtl2832u(&self, options: rtl2832u::Options) -> Result<Rtl2832u, Error> {
        let usb_device = self.usb.open().await?;

        const INTERFACE: u8 = 0;

        if options.detach_kernel_driver {
            usb_device.detach_kernel_driver(INTERFACE)?;
        }

        let usb_interface = usb_device.claim_interface(INTERFACE).await?;

        // create interface to RTL2832U device
        Ok(Rtl2832u::new(usb_interface, options.control_timeout))
    }
}

/// Defines how devices are detected.
///
/// By default [`BuiltinKnownDevices`] is used and should be sufficient. If you
/// need to detect a device with a non-standard USB vendor/product ID, you can
/// implement this and use [`EnumerateDevices::with_known_devices`].
pub trait KnownDevices {
    /// Check whether a device is an RTL2832U from the
    /// [`nusb::DeviceInfo`](https://docs.rs/nusb/latest/nusb/struct.DeviceInfo.html)
    /// and return the necessary information to use it.
    fn detect(&self, device_info: &nusb::DeviceInfo) -> Option<DeviceConfig>;
}

impl<T> KnownDevices for &T
where
    T: KnownDevices,
{
    fn detect(&self, device_info: &nusb::DeviceInfo) -> Option<DeviceConfig> {
        T::detect(self, device_info)
    }
}

/// Specific features of this device that we need to know about.
#[derive(Clone, derive_more::Debug)]
pub struct DeviceConfig {
    pub name: Cow<'static, str>,
    // todo: info about if it has a builtin upconverter, i.e. is a blog v4(l), etc.
}

/// Detects all builtin devices
///
/// Note that these devices are detected, but their tuners might not be
/// supported yet.
///
/// | Vendor ID | Product ID | Name                                                   |
/// |-----------|------------|--------------------------------------------------------|
/// | 0x0bda    | 0x2832     | Generic RTL2832U                                       |
/// | 0x0bda    | 0x2838     | Generic RTL2832U OEM                                   |
/// | 0x0413    | 0x6680     | DigitalNow Quad DVB-T PCI-E card                       |
/// | 0x0413    | 0x6f0f     | Leadtek WinFast DTV Dongle mini D                      |
/// | 0x0458    | 0x707f     | Genius TVGo DVB-T03 USB dongle (Ver. B)                |
/// | 0x0ccd    | 0x00a9     | Terratec Cinergy T Stick Black (rev 1)                 |
/// | 0x0ccd    | 0x00b3     | Terratec NOXON DAB/DAB+ USB dongle (rev 1)             |
/// | 0x0ccd    | 0x00b4     | Terratec Deutschlandradio DAB Stick                    |
/// | 0x0ccd    | 0x00b5     | Terratec NOXON DAB Stick - Radio Energy                |
/// | 0x0ccd    | 0x00b7     | Terratec Media Broadcast DAB Stick                     |
/// | 0x0ccd    | 0x00b8     | Terratec BR DAB Stick                                  |
/// | 0x0ccd    | 0x00b9     | Terratec WDR DAB Stick                                 |
/// | 0x0ccd    | 0x00c0     | Terratec MuellerVerlag DAB Stick                       |
/// | 0x0ccd    | 0x00c6     | Terratec Fraunhofer DAB Stick                          |
/// | 0x0ccd    | 0x00d3     | Terratec Cinergy T Stick RC (Rev.3)                    |
/// | 0x0ccd    | 0x00d7     | Terratec T Stick PLUS                                  |
/// | 0x0ccd    | 0x00e0     | Terratec NOXON DAB/DAB+ USB dongle (rev 2)             |
/// | 0x1554    | 0x5020     | PixelView PV-DT235U(RN)                                |
/// | 0x15f4    | 0x0131     | Astrometa DVB-T/DVB-T2                                 |
/// | 0x15f4    | 0x0133     | HanfTek DAB+FM+DVB-T                                   |
/// | 0x185b    | 0x0620     | Compro Videomate U620F                                 |
/// | 0x185b    | 0x0650     | Compro Videomate U650F                                 |
/// | 0x185b    | 0x0680     | Compro Videomate U680F                                 |
/// | 0x1b80    | 0xd393     | GIGABYTE GT-U7300                                      |
/// | 0x1b80    | 0xd394     | DIKOM USB-DVBT HD                                      |
/// | 0x1b80    | 0xd395     | Peak 102569AGPK                                        |
/// | 0x1b80    | 0xd397     | KWorld KW-UB450-T USB DVB-T Pico TV                    |
/// | 0x1b80    | 0xd398     | Zaapa ZT-MINDVBZP                                      |
/// | 0x1b80    | 0xd39d     | SVEON STV20 DVB-T USB & FM                             |
/// | 0x1b80    | 0xd3a4     | Twintech UT-40                                         |
/// | 0x1b80    | 0xd3a8     | ASUS U3100MINI_PLUS_V2                                 |
/// | 0x1b80    | 0xd3af     | SVEON STV27 DVB-T USB & FM                             |
/// | 0x1b80    | 0xd3b0     | SVEON STV21 DVB-T USB & FM                             |
/// | 0x1d19    | 0x1101     | Dexatek DK DVB-T Dongle (Logilink VG0002A)             |
/// | 0x1d19    | 0x1102     | Dexatek DK DVB-T Dongle (MSI DigiVox mini II V3.0)     |
/// | 0x1d19    | 0x1103     | Dexatek Technology Ltd. DK 5217 DVB-T Dongle           |
/// | 0x1d19    | 0x1104     | MSI DigiVox Micro HD                                   |
/// | 0x1f4d    | 0xa803     | Sweex DVB-T USB                                        |
/// | 0x1f4d    | 0xb803     | GTek T803                                              |
/// | 0x1f4d    | 0xc803     | Lifeview LV5TDeluxe                                    |
/// | 0x1f4d    | 0xd286     | MyGica TD312                                           |
/// | 0x1f4d    | 0xd803     | PROlectrix DV107669                                    |
#[derive(Clone, Debug, Default)]
pub struct BuiltinKnownDevices {
    devices: HashMap<(u16, u16), BuiltinKnownDevice>,
}

impl BuiltinKnownDevices {
    pub fn builtin() -> &'static Self {
        static ONCE: OnceLock<BuiltinKnownDevices> = OnceLock::new();
        ONCE.get_or_init(|| BUILTIN_KNOWN_DEVICES.iter().cloned().collect())
    }

    pub fn insert(&mut self, device: BuiltinKnownDevice) {
        self.devices
            .insert((device.vendor_id, device.product_id), device);
    }

    pub fn merge(&mut self, other: Self) {
        self.devices.extend(other.devices)
    }

    pub fn clear(&mut self) {
        self.devices.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &'_ BuiltinKnownDevice> {
        self.devices.values()
    }

    pub fn get(&self, vendor_id: u16, product_id: u16) -> Option<&BuiltinKnownDevice> {
        self.devices.get(&(vendor_id, product_id))
    }
}

impl FromIterator<BuiltinKnownDevice> for BuiltinKnownDevices {
    fn from_iter<T: IntoIterator<Item = BuiltinKnownDevice>>(iter: T) -> Self {
        Self {
            devices: iter
                .into_iter()
                .map(|device| ((device.vendor_id, device.product_id), device))
                .collect(),
        }
    }
}

impl KnownDevices for BuiltinKnownDevices {
    fn detect(&self, device_info: &nusb::DeviceInfo) -> Option<DeviceConfig> {
        let device = self.get(device_info.vendor_id(), device_info.product_id())?;

        match (
            device_info.manufacturer_string(),
            device_info.product_string(),
        ) {
            (Some("RTLSDRBlog"), Some("Blog V4")) => {
                // todo
            }
            (Some("RTLSDRBlog"), Some("Blog V4L")) => {
                // todo
            }
            _ => {}
        }

        Some(DeviceConfig {
            name: device.name.into(),
        })
    }
}

#[derive(Clone, Copy, derive_more::Debug)]
pub struct BuiltinKnownDevice {
    #[debug("0x{vendor_id:04x}")]
    pub vendor_id: u16,
    #[debug("0x{product_id:04x}")]
    pub product_id: u16,
    pub name: &'static str,
}

/// Build-time constant containing list of supported devices.
///
/// When you create a [`BuiltinKnownDevices`] this is turned into a hashmap for faster lookup (and stored for later use).
#[rustfmt::skip]
pub const BUILTIN_KNOWN_DEVICES: &[BuiltinKnownDevice] = &[
    BuiltinKnownDevice { vendor_id: 0x0bda, product_id: 0x2832, name: "Generic RTL2832U" },
    BuiltinKnownDevice { vendor_id: 0x0bda, product_id: 0x2838, name: "Generic RTL2832U OEM" },
    BuiltinKnownDevice { vendor_id: 0x0413, product_id: 0x6680, name: "DigitalNow Quad DVB-T PCI-E card" },
    BuiltinKnownDevice { vendor_id: 0x0413, product_id: 0x6f0f, name: "Leadtek WinFast DTV Dongle mini D" },
    BuiltinKnownDevice { vendor_id: 0x0458, product_id: 0x707f, name: "Genius TVGo DVB-T03 USB dongle (Ver. B)" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00a9, name: "Terratec Cinergy T Stick Black (rev 1)" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00b3, name: "Terratec NOXON DAB/DAB+ USB dongle (rev 1)" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00b4, name: "Terratec Deutschlandradio DAB Stick" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00b5, name: "Terratec NOXON DAB Stick - Radio Energy" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00b7, name: "Terratec Media Broadcast DAB Stick" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00b8, name: "Terratec BR DAB Stick" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00b9, name: "Terratec WDR DAB Stick" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00c0, name: "Terratec MuellerVerlag DAB Stick" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00c6, name: "Terratec Fraunhofer DAB Stick" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00d3, name: "Terratec Cinergy T Stick RC (Rev.3)" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00d7, name: "Terratec T Stick PLUS" },
    BuiltinKnownDevice { vendor_id: 0x0ccd, product_id: 0x00e0, name: "Terratec NOXON DAB/DAB+ USB dongle (rev 2)" },
    BuiltinKnownDevice { vendor_id: 0x1554, product_id: 0x5020, name: "PixelView PV-DT235U(RN)" },
    BuiltinKnownDevice { vendor_id: 0x15f4, product_id: 0x0131, name: "Astrometa DVB-T/DVB-T2" },
    BuiltinKnownDevice { vendor_id: 0x15f4, product_id: 0x0133, name: "HanfTek DAB+FM+DVB-T" },
    BuiltinKnownDevice { vendor_id: 0x185b, product_id: 0x0620, name: "Compro Videomate U620F" },
    BuiltinKnownDevice { vendor_id: 0x185b, product_id: 0x0650, name: "Compro Videomate U650F" },
    BuiltinKnownDevice { vendor_id: 0x185b, product_id: 0x0680, name: "Compro Videomate U680F" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd393, name: "GIGABYTE GT-U7300" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd394, name: "DIKOM USB-DVBT HD" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd395, name: "Peak 102569AGPK" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd397, name: "KWorld KW-UB450-T USB DVB-T Pico TV" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd398, name: "Zaapa ZT-MINDVBZP" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd39d, name: "SVEON STV20 DVB-T USB & FM" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd3a4, name: "Twintech UT-40" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd3a8, name: "ASUS U3100MINI_PLUS_V2" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd3af, name: "SVEON STV27 DVB-T USB & FM" },
    BuiltinKnownDevice { vendor_id: 0x1b80, product_id: 0xd3b0, name: "SVEON STV21 DVB-T USB & FM" },
    BuiltinKnownDevice { vendor_id: 0x1d19, product_id: 0x1101, name: "Dexatek DK DVB-T Dongle (Logilink VG0002A)" },
    BuiltinKnownDevice { vendor_id: 0x1d19, product_id: 0x1102, name: "Dexatek DK DVB-T Dongle (MSI DigiVox mini II V3.0)" },
    BuiltinKnownDevice { vendor_id: 0x1d19, product_id: 0x1103, name: "Dexatek Technology Ltd. DK 5217 DVB-T Dongle" },
    BuiltinKnownDevice { vendor_id: 0x1d19, product_id: 0x1104, name: "MSI DigiVox Micro HD" },
    BuiltinKnownDevice { vendor_id: 0x1f4d, product_id: 0xa803, name: "Sweex DVB-T USB" },
    BuiltinKnownDevice { vendor_id: 0x1f4d, product_id: 0xb803, name: "GTek T803" },
    BuiltinKnownDevice { vendor_id: 0x1f4d, product_id: 0xc803, name: "Lifeview LV5TDeluxe" },
    BuiltinKnownDevice { vendor_id: 0x1f4d, product_id: 0xd286, name: "MyGica TD312" },
    BuiltinKnownDevice { vendor_id: 0x1f4d, product_id: 0xd803, name: "PROlectrix DV107669" },
];
