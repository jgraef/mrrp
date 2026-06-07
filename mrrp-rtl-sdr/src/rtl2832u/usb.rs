use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::Duration,
};

use pin_project_lite::pin_project;
use tokio::io::{
    AsyncBufRead,
    AsyncRead,
    ReadBuf,
};

use crate::rtl2832u::{
    Error,
    register::Register,
};

pub const INTERFACE: u8 = 0;
pub const DATA_ENDPOINT: u8 = 0x81;

/// Wrapper around the USB interface that can be shared.
///
/// This is e.g. used by the main [`Rtl2832u`](super::Rtl2832u) device, and
/// [`I2cDevice`](super::i2c::I2cDevice)s.
#[derive(Clone, Debug)]
pub struct UsbInterface {
    usb_interface: nusb::Interface,
    control_timeout: Duration,
}

impl UsbInterface {
    pub fn new(usb_interface: nusb::Interface, control_timeout: Duration) -> Self {
        Self {
            usb_interface,
            control_timeout,
        }
    }

    pub async fn read(&mut self, address: Register, length: u16) -> Result<Vec<u8>, Error> {
        let request = address.control_in(length);

        tracing::trace!(?request, "sending control request");

        let response_data = self
            .usb_interface
            .control_in(request, self.control_timeout)
            .await
            .inspect_err(
                |error| tracing::error!(%error, ?address, ?length, "USB error during read"),
            )?;

        if response_data.len() != response_data.len() {
            return Err(Error::InvalidControlResponse {
                expected_length: length,
                response_length: response_data.len(),
            });
        }

        Ok(response_data)
    }

    pub async fn write(&mut self, address: Register, data: &[u8]) -> Result<(), Error> {
        let request = address.control_out(data);

        tracing::trace!(?request, "sending control request");

        self.usb_interface
            .control_out(request, self.control_timeout)
            .await
            .inspect_err(
                |error| tracing::error!(%error, ?address, ?data, "USB error during write"),
            )?;
        Ok(())
    }

    pub fn data_endpoint_reader(&mut self, buffer_size: usize) -> Result<Reader, Error> {
        let endpoint = self.usb_interface.endpoint(DATA_ENDPOINT)?;

        // buffer_size is the transfer_size from end EndpointRead docs.
        //
        // default num_transfer=1
        let endpoint_reader = endpoint.reader(buffer_size);

        Ok(Reader { endpoint_reader })
    }
}

pin_project! {
    pub struct Reader {
        #[pin]
        endpoint_reader: nusb::io::EndpointRead<nusb::transfer::Bulk>,
    }
}

impl Reader {
    pub fn set_num_transfers(&mut self, num_transfers: usize) {
        self.endpoint_reader.set_num_transfers(num_transfers);
    }
}

impl Debug for Reader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reader").finish_non_exhaustive()
    }
}

impl AsyncRead for Reader {
    #[inline(always)]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        self.project().endpoint_reader.poll_read(cx, buf)
    }
}

impl AsyncBufRead for Reader {
    #[inline(always)]
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        self.project().endpoint_reader.poll_fill_buf(cx)
    }

    #[inline(always)]
    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.endpoint_reader).consume(amt);
    }
}
