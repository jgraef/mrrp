use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::{
    sample::Complex,
    signal::{
        AsyncReadSamples,
        Frequency,
        GetSampleRate,
        ReadBuf,
        Remaining,
        StreamLength,
    },
};
use mrrp_rtl_sdr::{
    Device,
    Reader,
    device::ReaderOptions,
};
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    Converted,
};

use crate::multiplexer::source::{
    IntoSource,
    Source,
    SourceError,
};

#[derive(Debug)]
pub struct RtlSdr {
    device: Device,
    reader_options: ReaderOptions,
    reader: Option<Converted<Reader, Complex<f32>>>,
}

impl RtlSdr {
    pub fn new(device: Device, reader_options: ReaderOptions) -> Self {
        Self {
            device,
            reader_options,
            reader: None,
        }
    }
}

impl AsyncReadSamples for RtlSdr {
    type Sample = Complex<f32>;
    type Error = SourceError;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        if let Some(reader) = &mut self.reader {
            Pin::new(reader)
                .poll_read_samples(cx, buffer)
                .map_err(|error| Box::new(error) as _)
        }
        else {
            Poll::Ready(Ok(()))
        }
    }
}

impl StreamLength for RtlSdr {
    fn remaining(&self) -> Remaining {
        Remaining::Infinite
    }
}

impl GetSampleRate for RtlSdr {
    fn sample_rate(&self) -> Frequency {
        self.device.sample_rate()
    }
}

impl Source for RtlSdr {
    fn pause(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>> {
        Box::pin(async {
            if let Some(reader) = self.reader.take() {
                reader
                    .into_inner()
                    .close()
                    .await
                    .map_err(|error| Box::new(error) as SourceError)?;
            }
            Ok(())
        })
    }

    fn resume(&mut self) -> Pin<Box<dyn Future<Output = Result<(), Self::Error>> + Send + '_>> {
        Box::pin(async {
            if self.reader.is_none() {
                let reader = self
                    .device
                    .reader(self.reader_options.clone())
                    .await
                    .map_err(|error| Box::new(error) as SourceError)?;
                self.reader = Some(reader.convert());
            }
            Ok(())
        })
    }
}

impl IntoSource for Device {
    fn into_source(self) -> Box<dyn Source> {
        RtlSdr::new(self, Default::default()).into_source()
    }
}
