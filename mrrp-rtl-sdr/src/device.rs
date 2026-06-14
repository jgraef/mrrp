use std::{
    borrow::Cow,
    ops::{
        Deref,
        DerefMut,
    },
    pin::Pin,
    sync::Arc,
    task::{
        Context,
        Poll,
    },
};

use tokio::{
    io::{
        AsyncBufRead,
        AsyncRead,
        ReadBuf,
    },
    sync::{
        Mutex,
        MutexGuard,
    },
};

use crate::{
    Error,
    enumerate::DeviceInfo,
    rtl2832u::{
        self,
        EpaReader,
        IfMode,
        Rtl2832u,
        filter::FirFilter,
    },
    tuner::{
        AnyTuner,
        AnyTunerProbe,
        FallbackTunerProbe,
        IfSetting,
        Tuner,
        TunerProbe,
    },
};

#[derive(Clone, Debug)]
pub struct Options {
    pub reset_on_drop: bool,
    pub override_tuner_probe: Option<AnyTunerProbe>,
    pub fir_filter: FirFilter,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            reset_on_drop: true,
            override_tuner_probe: None,
            fir_filter: FirFilter::DEFAULT,
        }
    }
}

#[derive(Debug)]
pub struct Device {
    device_info: DeviceInfo,
    reset_on_drop: bool,

    /// The RTL2832U and tuner are in an `Arc<Mutex<_>>` so we can access them
    /// in drop code for [`Device`] and [`Reader`].
    inner: SharedInner,

    /// The frequency correction factor (in ppm) that was set on the
    /// RTL2832U.
    frequency_correction: i16,

    /// The crystal frequency (in Hz) that was set on the
    /// RTL2832U.
    rtl_crystal_frequency: f32,

    /// The current sample rate
    sample_rate: f32,

    /// The current center frequency
    center_frequency: Option<f32>,
}

impl Device {
    pub async fn from_rtl2832u(
        mut rtl2832u: Rtl2832u,
        device_info: DeviceInfo,
        options: Options,
    ) -> Result<Self, Error> {
        // todo: should we try to reset the device if initialization fails?

        // initialize baseband
        rtl2832u.initialize(&options.fir_filter).await?;

        let rtl_crystal_frequency = rtl2832u::DEFAULT_CRYSTAL_FREQUENCY as f32;

        // probe tuners
        let mut i2c_repeater_guard = rtl2832u.enable_i2c_repeater().await?;

        // either use the override from options, or the one provided by the device
        // config, or the fallback - in that order.
        let tuner_probe = options
            .override_tuner_probe
            .as_ref()
            .or(device_info.device_config.tuner_probe.as_ref())
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(AnyTunerProbe::new(FallbackTunerProbe)));

        let tuner = tuner_probe
            .try_open(&mut i2c_repeater_guard)
            .await?
            .ok_or(Error::NoTunerFound)?;

        i2c_repeater_guard.disable().await?;

        tracing::info!(tuner = tuner.name(), "found tuner");

        let mut inner = Inner { rtl2832u, tuner };

        // initialize IF on rtl2832u
        inner.configure_if().await?;

        // get the initial sample rate
        let sample_rate = inner
            .rtl2832u
            .get_sample_rate(rtl_crystal_frequency)
            .await?;
        tracing::debug!(?sample_rate, "initial sample rate");

        // todo: we can't really figure out the initial center frequency, because e.g.
        // the R82xx doesn't let us read the relevant registers.
        //
        // we have considered writing the center frequency into unused rtl2832u's system
        // memory memory. we would have to make sure that this memory is absolutely
        // unused - which is hard, or impossible.

        Ok(Self {
            device_info,
            reset_on_drop: options.reset_on_drop,
            inner: SharedInner {
                inner: Arc::new(Mutex::new(inner)),
            },
            frequency_correction: 0,
            rtl_crystal_frequency,
            sample_rate,
            center_frequency: None,
        })
    }

    /// Properly close the device.
    ///
    /// This resets the device. By default the Drop handler for [`Device`] will
    /// also attempt to do this. But it needs to spawn a tokio task for this and
    /// that task might never run, if e.g. the tokio runtime is shutdown
    /// immediately after.
    ///
    /// Not resetting the device leaves it running, which consumes more power.
    pub async fn close(mut self) -> Result<(), Error> {
        tracing::debug!("closing device");
        self.reset_on_drop = false;
        let mut inner = self.inner_mut().await;
        inner.reset().await?;
        Ok(())
    }

    #[inline(always)]
    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    #[inline(always)]
    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    #[inline(always)]
    pub fn center_frequency(&self) -> Option<f32> {
        self.center_frequency
    }

    /// todo: pub for testing only
    pub async fn inner_mut(&mut self) -> InnerGuard<'_> {
        self.inner.lock().await
    }

    pub async fn reader(&mut self, buffer_size: usize) -> Result<Reader, Error> {
        let mut inner = self.inner.lock().await;
        inner.rtl2832u.start_epa().await?;
        let epa_reader = inner.rtl2832u.epa_reader(buffer_size)?;

        Ok(Reader {
            epa_reader,
            inner: self.inner.clone(),
            stop_on_drop: true,
        })
    }

    /// Set the frequency correction factor in ppm.
    ///
    /// This is a 14-bit signed integer, thus must be between -8192 and 8191
    /// inclusive.
    pub async fn set_frequency_correction(
        &mut self,
        frequency_correction: i16,
    ) -> Result<(), Error> {
        if self.frequency_correction != frequency_correction {
            assert!(
                frequency_correction >= -8192 && frequency_correction <= 8191,
                "frequency_correction must be between -8192 and 8191 inclusive: {frequency_correction}"
            );

            let Inner { rtl2832u, tuner: _ } = &mut *self.inner.lock().await;

            rtl2832u
                .set_sample_frequency_correction(frequency_correction)
                .await?;

            self.frequency_correction = frequency_correction;
        }

        Ok(())
    }

    pub async fn set_sample_rate(&mut self, sample_rate: f32) -> Result<(), Error> {
        tracing::debug!(?sample_rate, "setting sample rate");

        let inner = &mut *self.inner.lock().await;

        // set rtl2832u's sample rate.
        //
        // this returns the sample rate that we actually get.
        let actual_sample_rate = inner
            .rtl2832u
            .set_sample_rate(sample_rate as f32, self.rtl_crystal_frequency)
            .await?;
        tracing::debug!(?actual_sample_rate);

        {
            let mut i2c_repeater_guard = inner.rtl2832u.enable_i2c_repeater().await?;

            // configure tuner for the actual sample rate we have.
            inner
                .tuner
                .set_bandwidth(&mut *i2c_repeater_guard, actual_sample_rate)
                .await?;

            // after changing the tuner bandwidth, its if frequency changes, which means
            // we're not tuned correctly anymore.
            if let Some(center_frequency) = self.center_frequency {
                inner
                    .tuner
                    .set_center_frequency(&mut *&mut i2c_repeater_guard, center_frequency)
                    .await?;
            }

            i2c_repeater_guard.disable().await?;
        }

        // set rtl2832u's if frequency, because tuner can change this when changing
        // bandwidth.
        inner.configure_if().await?;

        self.sample_rate = actual_sample_rate;

        Ok(())
    }

    pub async fn set_center_frequency(&mut self, center_frequency: f32) -> Result<(), Error> {
        tracing::debug!(?center_frequency, "setting center frequency");

        let Inner { rtl2832u, tuner } = &mut *self.inner.lock().await;

        // librtlsdr sets the "exact" sample rate here. We think they basically convert
        // from the encoded value back to Hz. But they also do some bit-manipulation.
        {
            let mut i2c_repeater_guard = rtl2832u.enable_i2c_repeater().await?;

            tuner
                .set_center_frequency(&mut *i2c_repeater_guard, center_frequency)
                .await?;

            i2c_repeater_guard.disable().await?;
        }

        self.center_frequency = Some(center_frequency);

        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if self.reset_on_drop {
            tracing::warn!("Device dropped without closing. Attempting to reset the device.");

            self.reset_on_drop = false;

            let inner = self.inner.clone();

            tokio::spawn(async move {
                let mut inner = inner.lock().await;

                if let Err(error) = inner.reset().await {
                    tracing::error!(%error, "Error resetting RTL2832U while dropping");
                }
            });
        }
    }
}

/// Reads data from the device.
///
/// Usually the data comes in pair of bytes - the I Q values of the samples.
///
/// You can configure the device to only return either the I or Q ADC output, in
/// which case only one byte per sample is returned.
///
/// By default this will try to disable the data endpoint on drop. It does this
/// by spawning a tokio task, since it requires async context. This sometimes
/// fails if the tokio runtime is shutdown immediately after. Try to avoid this
/// by explicitely closing the reader via the [`close`](Self::close) method.
/// Otherwise you can also disable this behavior via the
/// [`disam_stop_on_drop`](Self::disarm_stop_on_drop) method.
///
/// # TODO
///
/// Should this map a device stalled error to EOF?
#[derive(Debug)]
pub struct Reader {
    epa_reader: EpaReader,
    inner: SharedInner,
    stop_on_drop: bool,
}

impl Reader {
    /// Sets the number of concurrent USB transfers.
    ///
    /// Refer to the documentation of [`nusb::io::EndpointRead`] for more
    /// information.
    pub fn set_num_transfers(&mut self, num_transfers: usize) {
        self.epa_reader.set_num_transfers(num_transfers);
    }

    /// Disable the default drop behavior.
    ///
    /// By default EPA is reset and stalled on drop, and is likely what you
    /// want. This lets you override this behavior.
    pub fn disarm_stop_on_drop(&mut self) {
        self.stop_on_drop = false;
    }

    pub async fn close(mut self) -> Result<(), Error> {
        self.stop_on_drop = false;
        let mut inner = self.inner.lock().await;
        inner.rtl2832u.stop_epa().await?;
        Ok(())
    }
}

impl AsyncRead for Reader {
    #[inline(always)]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<Result<(), std::io::Error>> {
        Pin::new(&mut self.get_mut().epa_reader).poll_read(cx, buf)
    }
}

impl AsyncBufRead for Reader {
    #[inline(always)]
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        Pin::new(&mut self.get_mut().epa_reader).poll_fill_buf(cx)
    }

    #[inline(always)]
    fn consume(self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.get_mut().epa_reader).consume(amt);
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        if self.stop_on_drop {
            self.stop_on_drop = false;

            let inner = self.inner.clone();
            tokio::spawn(async move {
                let mut inner = inner.lock().await;

                if let Err(error) = inner.rtl2832u.stop_epa().await {
                    tracing::error!(%error, "Error stopping EPA while dropping");
                }
            });
        }
    }
}

/// todo: pub only for testing
#[derive(Debug)]
pub struct Inner {
    pub rtl2832u: Rtl2832u,
    pub tuner: AnyTuner,
}

impl Inner {
    async fn reset(&mut self) -> Result<(), Error> {
        // reset tuner
        let mut i2c_repeater_guard = self.rtl2832u.enable_i2c_repeater().await?;
        self.tuner.shutdown(&mut *i2c_repeater_guard).await?;
        i2c_repeater_guard.disable().await?;

        // reset rtl2832u
        self.rtl2832u.reset(Default::default()).await?;

        Ok(())
    }

    async fn configure_if(&mut self) -> Result<(), Error> {
        let if_setting = self.tuner.if_setting();

        tracing::debug!(?if_setting, "setting IF");

        match if_setting {
            IfSetting::ZeroIf => {
                self.rtl2832u.set_if_mode(IfMode::ZeroIf).await?;

                // todo: do we have to set the IF frequency or spectrum
                // inversion here? probably not the IF frequency, but maybe the
                // spectrum can still be inverted?
            }
            IfSetting::If {
                frequency,
                invert_spectrum,
            } => {
                self.rtl2832u.set_if_mode(IfMode::If).await?;

                self.rtl2832u
                    .set_if_frequency(frequency, rtl2832u::DEFAULT_CRYSTAL_FREQUENCY as f32)
                    .await?;

                self.rtl2832u
                    .enable_spectrum_inversion(invert_spectrum)
                    .await?;
            }
        }

        Ok(())
    }
}

#[derive(Clone, Debug)]
struct SharedInner {
    inner: Arc<Mutex<Inner>>,
}

impl SharedInner {
    pub async fn lock(&self) -> InnerGuard<'_> {
        let guard = self.inner.lock().await;

        InnerGuard { guard }
    }
}

/// todo: pub only for testing
pub struct InnerGuard<'a> {
    guard: MutexGuard<'a, Inner>,
}

impl<'a> Deref for InnerGuard<'a> {
    type Target = Inner;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &*self.guard
    }
}

impl<'a> DerefMut for InnerGuard<'a> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.guard
    }
}

/// Applies `correction` (in PPM) to `frequency` (in Hz).
#[inline(always)]
pub fn apply_frequency_correction(frequency: u32, correction: i16) -> u32 {
    // todo: check for overflow?
    (frequency as f32 * (1.0 + correction as f32 / 1.0e6)) as u32
}
