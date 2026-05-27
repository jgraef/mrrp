use anyhow::Error;

use crate::config::{
    RadioConfig,
    RtlSdrDeviceFilter,
};

pub fn list_devices() -> Result<impl Iterator<Item = RadioDescriptor>, Error> {
    let rtl_sdr_list = rtl_sdr_rs::RtlSdr::list_devices()?
        .into_iter()
        .map(RadioDescriptor::RtlSdr);

    Ok(rtl_sdr_list)
}

#[derive(Clone, Debug)]
pub enum RadioDescriptor {
    RtlSdr(rtl_sdr_rs::DeviceDescriptor),
}

impl RadioDescriptor {
    pub fn matches(&self, config: &RadioConfig) -> bool {
        match (self, config) {
            (RadioDescriptor::RtlSdr(descriptor), RadioConfig::RtlSdr { filter, .. }) => {
                match_rtl_sdr(descriptor, filter)
            }
            _ => false,
        }
    }

    pub fn open(&self) -> Result<Radio, Error> {
        let radio = match self {
            RadioDescriptor::RtlSdr(device_descriptor) => {
                Radio::RtlSdr(rtl_sdr_rs::RtlSdr::open_with_index(
                    device_descriptor.index,
                )?)
            }
        };

        Ok(radio)
    }

    pub fn name(&self) -> String {
        match self {
            RadioDescriptor::RtlSdr(device_descriptor) => {
                format!(
                    "{} {} ({})",
                    device_descriptor.manufacturer,
                    device_descriptor.product,
                    device_descriptor.serial
                )
            }
        }
    }
}

fn match_rtl_sdr(descriptor: &rtl_sdr_rs::DeviceDescriptor, filter: &RtlSdrDeviceFilter) -> bool {
    // looks like we don't want that, because we'll show all detected, but
    // unconfigured devices anyway
    /*if filter.index.is_none()
        && filter.vendor_id.is_none()
        && filter.product_id.is_none()
        && filter.manufacturer.is_none()
        && filter.product.is_none()
        && filter.serial.is_none()
    {
        // if nothing is defined in the filter, we always match
        return true;
    }*/

    filter
        .index
        .map_or(false, |index| index == descriptor.index)
        && filter
            .vendor_id
            .map_or(false, |vendor_id| vendor_id == descriptor.vendor_id)
        && filter
            .product_id
            .map_or(false, |product_id| product_id == descriptor.product_id)
        && filter.manufacturer.as_ref().map_or(false, |manufacturer| {
            manufacturer == &descriptor.manufacturer
        })
        && filter
            .product
            .as_ref()
            .map_or(false, |product| product == &descriptor.product)
        && filter
            .serial
            .as_ref()
            .map_or(false, |serial| serial == &descriptor.serial)
}

#[derive(derive_more::Debug)]
pub enum Radio {
    RtlSdr(#[debug(skip)] rtl_sdr_rs::RtlSdr),
}
