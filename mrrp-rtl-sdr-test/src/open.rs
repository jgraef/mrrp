use anyhow::{
    Error,
    anyhow,
};
use mrrp_rtl_sdr::{
    Device,
    DeviceInfo,
    rtl2832u::Rtl2832u,
};

pub async fn find_device(serial: Option<&str>) -> Result<DeviceInfo, Error> {
    for device_info in mrrp_rtl_sdr::enumerate_devices().await? {
        if serial.is_none() || device_info.serial_number() == serial {
            tracing::debug!(?device_info, "device found");
            return Ok(device_info);
        }
    }

    if let Some(serial) = serial {
        Err(anyhow!("Device not found: {serial}"))
    }
    else {
        Err(anyhow!("No device found"))
    }
}

pub async fn open_rtl2832u(serial: Option<&str>) -> Result<(Rtl2832u, DeviceInfo), Error> {
    let device_info = find_device(serial).await?;

    let rtl2832u = device_info.open_rtl2832u(Default::default()).await?;

    Ok((rtl2832u, device_info))
}

pub async fn open_device(serial: Option<&str>) -> Result<Device, Error> {
    Ok(find_device(serial).await?.open(Default::default()).await?)
}
