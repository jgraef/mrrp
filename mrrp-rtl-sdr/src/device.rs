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
        gain::IntoTunerGain,
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

/// A RTL-SDR device.
///
/// This is most likely the object you want to work with. It represents the
/// combination of a [`Rtl2832u`] and a tuner chip. This will automatically
/// discover which tuner to use. It will also handle some specific
/// configurations, like the RTL-SDR Blog.
///
/// The intended way to create a [`Device`] is with
/// [`DeviceInfo::open`][crate::enumerate::DeviceInfo::open]. The
/// [`DeviceInfo`] itself can be retrieved with
/// [`enumerate_devices`](crate::enumerate::enumerate_devices). Refer to the
/// [`crate level`](crate) documentation for examples.
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

    /// The current sample rate
    sample_rate: f32,

    /// The current center frequency
    center_frequency: Option<f32>,

    /// Gain values in dB that are available with the tuner.
    tuner_gains: Vec<f32>,

    if_offset: f32,
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

        let tuner_gains = tuner.gains().to_vec();
        tracing::debug!(?tuner_gains);

        tracing::info!(tuner = tuner.name(), "found tuner");

        let mut inner = Inner { rtl2832u, tuner };

        // initialize IF on rtl2832u
        inner.configure_if(0.0).await?;

        // get the initial sample rate
        let sample_rate = inner.rtl2832u.sample_rate().await?;
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
            sample_rate,
            center_frequency: None,
            tuner_gains,
            if_offset: 0.0,
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

    /// Provides access to the [`Rtl2832u`] and [tuner](AnyTuner).
    ///
    /// This acquires a lock, so should be held for as short as possible. The
    /// lock is necessary because this inner struct is shared with [`Reader`]
    /// for configuring USB on drop (i.e. it stalls endpoint A).
    ///
    /// # TODO
    ///
    /// Do we want to expose this? I'd prefer if you can actually somehow access
    /// the [`Rtl2832u`] and tuner if possible. Especially the tuner is
    /// important, as then you can downcast it to a specific tuner and use
    /// settings that are not exposed via the trait.
    ///
    /// Also technically this doesn't need mutual ownership of the device, but
    /// we think it makes sense to have it that way, as the Device is not
    /// supposed to allow shared usage (except for the before-mentioned drop
    /// handlers).
    #[inline(always)]
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
    ///
    /// # TODO
    ///
    /// Not public atm, because it's completely untested. Also the argument
    /// should probably be `f32` and just be clamped and rounded.
    #[allow(dead_code)]
    async fn set_frequency_correction(&mut self, frequency_correction: i16) -> Result<(), Error> {
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
        let actual_sample_rate = inner.rtl2832u.set_sample_rate(sample_rate as f32).await?;
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
        inner.configure_if(self.if_offset).await?;

        // librtlsdr sets the sample frequency correction here too. we don't support
        // changing this yet, but this will set the two most significant bits to 0.
        // these are unknown but librtlsdr sets them to 0 while doing this, and they
        // start out as 0b10.
        inner
            .rtl2832u
            .set_sample_frequency_correction(self.frequency_correction)
            .await?;

        // librtlsdr performs a soft-reset at the end
        //
        // the bug with the frequency shift was probably because we did this in
        // `Rtl2832u::set_sample_rate`, and only if the value changed.
        // we think this is actually necessary to apply the sample frequency correction.
        // though we set it to 0, which is initially, we clear out a bit in that
        // register. we think that bit might be automatic sample rate correction
        // (however that would work), but the soft-reset needs to be done afterwards.
        inner.rtl2832u.set_soft_reset(true).await?;
        inner.rtl2832u.set_soft_reset(false).await?;

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

    pub async fn set_if_offset(&mut self, if_offset: f32) -> Result<(), Error> {
        tracing::debug!(?if_offset, "setting IF offset");

        self.if_offset = if_offset;

        let inner = &mut *self.inner.lock().await;
        inner.configure_if(if_offset).await?;

        Ok(())
    }

    /// Enables or disables the [`Rtl2832u`]'s Digital Automatic Gain Control.
    pub async fn set_agc_mode(&mut self, enable: bool) -> Result<(), Error> {
        tracing::debug!(?enable, "enable DAGC mode");
        let Inner { rtl2832u, tuner: _ } = &mut *self.inner.lock().await;
        rtl2832u.set_agc_mode(enable).await?;
        Ok(())
    }

    /// Sets the tuner's gain.
    ///
    /// This can be either [`Auto`](crate::tuner::gain::Auto),
    /// [`Index(usize)`](crate::tuner::gain::Index),
    /// [`Db(f32)`](crate::tuner::gain::Db), or the enum these convert into,
    /// [`TunerGain`](crate::tuner::gain::TunerGain).
    pub async fn set_tuner_gain(&mut self, gain: impl IntoTunerGain) -> Result<(), Error> {
        let gain = gain.into_tuner_gain(&self.tuner_gains);
        tracing::debug!(?gain, "set tuner gain");

        let Inner { rtl2832u, tuner } = &mut *self.inner.lock().await;

        {
            let mut i2c_repeater_guard = rtl2832u.enable_i2c_repeater().await?;

            tuner.set_gain(&mut *i2c_repeater_guard, gain).await?;

            i2c_repeater_guard.disable().await?;
        }

        Ok(())
    }

    #[inline(always)]
    pub fn available_tuner_gains(&self) -> &[f32] {
        &self.tuner_gains
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
/// [`disarm_stop_on_drop`](Self::disarm_stop_on_drop) method.
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

    async fn configure_if(&mut self, mut if_offset: f32) -> Result<(), Error> {
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

                if invert_spectrum {
                    if_offset *= -1.0;
                }

                self.rtl2832u
                    .set_if_frequency(frequency + if_offset)
                    .await?;

                self.rtl2832u
                    .set_spectrum_inversion(invert_spectrum)
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
///
/// # TODO
///
/// This is dead code right now, as we don't apply any frequency correction at
/// the moment.
#[inline(always)]
pub fn apply_frequency_correction(frequency: f32, correction: f32) -> f32 {
    frequency * (1.0 + correction / 1.0e6)
}
