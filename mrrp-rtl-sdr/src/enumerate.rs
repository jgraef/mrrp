use std::{
    borrow::Cow,
    collections::HashMap,
    sync::OnceLock,
};

use crate::{
    Device,
    Error,
    OpenOptions,
};

pub async fn enumerate_devices() -> Result<EnumerateDevices<'static>, Error> {
    Ok(EnumerateDevices {
        devices: Box::new(nusb::list_devices().await?),
        known_devices: KnownDevices::builtin(),
    })
}

#[derive(derive_more::Debug)]
pub struct EnumerateDevices<'a> {
    #[debug(skip)]
    devices: Box<dyn Iterator<Item = nusb::DeviceInfo>>,

    known_devices: &'a KnownDevices,
}

impl<'a> EnumerateDevices<'a> {
    pub fn with_known_devices<'b>(self, known_devices: &'b KnownDevices) -> EnumerateDevices<'b> {
        EnumerateDevices {
            devices: self.devices,
            known_devices,
        }
    }
}

impl<'a> Iterator for EnumerateDevices<'a> {
    type Item = DeviceInfo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let device_info = self.devices.next()?;

            if let Some(known_device) = self
                .known_devices
                .get(device_info.vendor_id(), device_info.product_id())
            {
                return Some(DeviceInfo {
                    usb: device_info,
                    known_device: known_device.clone(),
                });
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.devices.size_hint()
    }
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub(crate) usb: nusb::DeviceInfo,
    pub(crate) known_device: KnownDevice,
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

    pub fn known_device(&self) -> &KnownDevice {
        &self.known_device
    }

    pub async fn open(self, options: OpenOptions) -> Result<Device, Error> {
        Device::open(self, options).await
    }
}

#[derive(Clone, derive_more::Debug)]
pub struct KnownDevice {
    #[debug("0x{vendor_id:04x}")]
    pub vendor_id: u16,
    #[debug("0x{product_id:04x}")]
    pub product_id: u16,
    pub name: Cow<'static, str>,
}

#[derive(Clone, Debug, Default)]
pub struct KnownDevices {
    devices: HashMap<(u16, u16), KnownDevice>,
}

impl KnownDevices {
    pub fn builtin() -> &'static Self {
        static ONCE: OnceLock<KnownDevices> = OnceLock::new();
        ONCE.get_or_init(|| BUILTIN_KNOWN_DEVICES.iter().cloned().collect())
    }

    pub fn insert(&mut self, device: KnownDevice) {
        self.devices
            .insert((device.vendor_id, device.product_id), device);
    }

    pub fn merge(&mut self, other: Self) {
        self.devices.extend(other.devices)
    }

    pub fn clear(&mut self) {
        self.devices.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = &'_ KnownDevice> {
        self.devices.values()
    }

    pub fn get(&self, vendor_id: u16, product_id: u16) -> Option<&KnownDevice> {
        self.devices.get(&(vendor_id, product_id))
    }
}

impl FromIterator<KnownDevice> for KnownDevices {
    fn from_iter<T: IntoIterator<Item = KnownDevice>>(iter: T) -> Self {
        Self {
            devices: iter
                .into_iter()
                .map(|device| ((device.vendor_id, device.product_id), device))
                .collect(),
        }
    }
}

#[rustfmt::skip]
pub const BUILTIN_KNOWN_DEVICES: &[KnownDevice] = &[
    KnownDevice { vendor_id: 0x0bda, product_id: 0x2832, name: Cow::Borrowed("Generic RTL2832U") },
	KnownDevice { vendor_id: 0x0bda, product_id: 0x2838, name: Cow::Borrowed("Generic RTL2832U OEM") },
	KnownDevice { vendor_id: 0x0413, product_id: 0x6680, name: Cow::Borrowed("DigitalNow Quad DVB-T PCI-E card") },
	KnownDevice { vendor_id: 0x0413, product_id: 0x6f0f, name: Cow::Borrowed("Leadtek WinFast DTV Dongle mini D") },
	KnownDevice { vendor_id: 0x0458, product_id: 0x707f, name: Cow::Borrowed("Genius TVGo DVB-T03 USB dongle (Ver. B)") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00a9, name: Cow::Borrowed("Terratec Cinergy T Stick Black (rev 1)") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00b3, name: Cow::Borrowed("Terratec NOXON DAB/DAB+ USB dongle (rev 1)") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00b4, name: Cow::Borrowed("Terratec Deutschlandradio DAB Stick") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00b5, name: Cow::Borrowed("Terratec NOXON DAB Stick - Radio Energy") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00b7, name: Cow::Borrowed("Terratec Media Broadcast DAB Stick") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00b8, name: Cow::Borrowed("Terratec BR DAB Stick") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00b9, name: Cow::Borrowed("Terratec WDR DAB Stick") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00c0, name: Cow::Borrowed("Terratec MuellerVerlag DAB Stick") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00c6, name: Cow::Borrowed("Terratec Fraunhofer DAB Stick") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00d3, name: Cow::Borrowed("Terratec Cinergy T Stick RC (Rev.3)") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00d7, name: Cow::Borrowed("Terratec T Stick PLUS") },
	KnownDevice { vendor_id: 0x0ccd, product_id: 0x00e0, name: Cow::Borrowed("Terratec NOXON DAB/DAB+ USB dongle (rev 2)") },
	KnownDevice { vendor_id: 0x1554, product_id: 0x5020, name: Cow::Borrowed("PixelView PV-DT235U(RN)") },
	KnownDevice { vendor_id: 0x15f4, product_id: 0x0131, name: Cow::Borrowed("Astrometa DVB-T/DVB-T2") },
	KnownDevice { vendor_id: 0x15f4, product_id: 0x0133, name: Cow::Borrowed("HanfTek DAB+FM+DVB-T") },
	KnownDevice { vendor_id: 0x185b, product_id: 0x0620, name: Cow::Borrowed("Compro Videomate U620F") },
	KnownDevice { vendor_id: 0x185b, product_id: 0x0650, name: Cow::Borrowed("Compro Videomate U650F") },
	KnownDevice { vendor_id: 0x185b, product_id: 0x0680, name: Cow::Borrowed("Compro Videomate U680F") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd393, name: Cow::Borrowed("GIGABYTE GT-U7300") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd394, name: Cow::Borrowed("DIKOM USB-DVBT HD") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd395, name: Cow::Borrowed("Peak 102569AGPK") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd397, name: Cow::Borrowed("KWorld KW-UB450-T USB DVB-T Pico TV") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd398, name: Cow::Borrowed("Zaapa ZT-MINDVBZP") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd39d, name: Cow::Borrowed("SVEON STV20 DVB-T USB & FM") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd3a4, name: Cow::Borrowed("Twintech UT-40") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd3a8, name: Cow::Borrowed("ASUS U3100MINI_PLUS_V2") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd3af, name: Cow::Borrowed("SVEON STV27 DVB-T USB & FM") },
	KnownDevice { vendor_id: 0x1b80, product_id: 0xd3b0, name: Cow::Borrowed("SVEON STV21 DVB-T USB & FM") },
	KnownDevice { vendor_id: 0x1d19, product_id: 0x1101, name: Cow::Borrowed("Dexatek DK DVB-T Dongle (Logilink VG0002A)") },
	KnownDevice { vendor_id: 0x1d19, product_id: 0x1102, name: Cow::Borrowed("Dexatek DK DVB-T Dongle (MSI DigiVox mini II V3.0)") },
	KnownDevice { vendor_id: 0x1d19, product_id: 0x1103, name: Cow::Borrowed("Dexatek Technology Ltd. DK 5217 DVB-T Dongle") },
	KnownDevice { vendor_id: 0x1d19, product_id: 0x1104, name: Cow::Borrowed("MSI DigiVox Micro HD") },
	KnownDevice { vendor_id: 0x1f4d, product_id: 0xa803, name: Cow::Borrowed("Sweex DVB-T USB") },
	KnownDevice { vendor_id: 0x1f4d, product_id: 0xb803, name: Cow::Borrowed("GTek T803") },
	KnownDevice { vendor_id: 0x1f4d, product_id: 0xc803, name: Cow::Borrowed("Lifeview LV5TDeluxe") },
	KnownDevice { vendor_id: 0x1f4d, product_id: 0xd286, name: Cow::Borrowed("MyGica TD312") },
	KnownDevice { vendor_id: 0x1f4d, product_id: 0xd803, name: Cow::Borrowed("PROlectrix DV107669") },
];
