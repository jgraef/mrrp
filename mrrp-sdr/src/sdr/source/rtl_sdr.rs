use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use anyhow::Error;
use mrrp::{
    buf::SampleBufMut,
    io::{
        AsyncReadSamples,
        ReadBuf,
        Remaining,
        StreamLength,
    },
    sample::Sample,
};
use num_complex::Complex;
use tokio::io::AsyncBufRead;

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
    reader: Option<mrrp_rtl_sdr::Reader>,
    name: String,
    buffer_size: usize,
}

impl RtlSdrSource {
    pub fn new(device: mrrp_rtl_sdr::Device) -> Self {
        let name = format!("RTL-SDR - {}", device.device_info().name());

        Self {
            device,
            reader: None,
            name,
            // 1 MiB
            // todo: if we make this configurable, it must be larger than 1 sample in bytes
            buffer_size: 0x100000,
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
                self.reader = Some(self.device.reader(self.buffer_size).await?);
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
}

impl AsyncReadSamples<Iq> for RtlSdrSource {
    type Error = Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        read_buf: &mut ReadBuf<Iq>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = &mut *self;

        if let Some(reader) = &mut this.reader {
            match Pin::new(&mut *reader).poll_fill_buf(cx) {
                Poll::Pending => Poll::Pending,
                Poll::Ready(Err(error)) => Poll::Ready(Err(error.into())),
                Poll::Ready(Ok(buffer)) => {
                    // ignore last byte if it's odd-numbered
                    let n = buffer.len() & !1;
                    assert_ne!(n, 0);

                    let samples = bytemuck::cast_slice::<_, Complex<u8>>(&buffer[..n]);

                    let mut i = 0;
                    while read_buf.has_remaining_mut() && i < n {
                        read_buf.put_sample(samples[i].into_float());
                        i += 1;
                    }

                    Pin::new(reader).consume(i * size_of::<Complex<u8>>());

                    Poll::Ready(Ok(()))
                }
            }
        }
        else {
            Poll::Ready(Ok(()))
        }
    }
}

impl StreamLength for RtlSdrSource {
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }
}
