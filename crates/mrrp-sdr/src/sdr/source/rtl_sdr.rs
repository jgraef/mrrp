use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use anyhow::Error;
use futures_util::TryFutureExt;
use mrrp_core::signal::{
    AsyncReadSamples,
    ReadBuf,
    Remaining,
    StreamLength,
};
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    Converted,
};

use crate::sdr::{
    Iq,
    source::{
        IntoSource,
        Source,
    },
};

impl IntoSource for mrrp_rtl_sdr::Device {
    type Source = RtlSdrSource;

    fn into_source(self) -> Self::Source {
        RtlSdrSource::new(self)
    }
}

pub struct RtlSdrSource {
    device: mrrp_rtl_sdr::Device,
    // todo: `Converted` buffers internally, but it could read directly from the Reader buffer
    // instread, if we had AsyncBufReadSamples. Alternatively we could implement
    // AsyncReadSamples<Sample = Complex<f32>> direclty on the Reader, since it's such a common use
    // case.
    reader: Option<Converted<mrrp_rtl_sdr::Reader, Iq>>,
    name: String,
}

impl RtlSdrSource {
    pub fn new(device: mrrp_rtl_sdr::Device) -> Self {
        let name = format!("RTL-SDR - {}", device.device_info().name());

        Self {
            device,
            reader: None,
            name,
        }
    }
}

impl Source for RtlSdrSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn center_frequency(&self) -> f32 {
        // todo
        7000000.0
    }

    fn sample_rate(&self) -> f32 {
        // todo
        2400000.0
    }

    fn start(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(async {
            if self.reader.is_none() {
                let reader = self.device.reader(Default::default()).await?;
                self.reader = Some(reader.convert());
            }
            Ok(())
        })
    }

    fn stop(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(async {
            self.reader = None;
            Ok(())
        })
    }

    fn close(self: Pin<Box<Self>>) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send>> {
        Box::pin(async move {
            Pin::into_inner(self).device.close().await?;
            Ok(())
        })
    }

    fn set_sample_rate(
        &mut self,
        sample_rate: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(self.device.set_sample_rate(sample_rate).map_err(Into::into))
    }

    fn set_center_frequency(
        &mut self,
        center_frequency: f32,
    ) -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send + '_>> {
        Box::pin(
            self.device
                .set_center_frequency(center_frequency)
                .map_err(Into::into),
        )
    }
}

impl AsyncReadSamples for RtlSdrSource {
    type Sample = Iq;
    type Error = Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Iq>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = &mut *self;

        if let Some(reader) = &mut this.reader {
            Pin::new(reader)
                .poll_read_samples(cx, buffer)
                .map_err(Into::into)
        }
        else {
            // todo: we can remove this when if we split the sample stream from the control
            // interface
            panic!("Sample stream not started");
        }
    }
}

impl StreamLength for RtlSdrSource {
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }
}
