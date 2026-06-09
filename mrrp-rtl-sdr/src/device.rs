use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use tokio::io::{
    AsyncBufRead,
    AsyncRead,
    ReadBuf,
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
        Tuner,
        TunerProbe,
        r82xx,
    },
};

#[derive(Clone, Debug)]
pub struct Options {
    pub reset_on_drop: bool,
    pub tuner_probe: AnyTunerProbe,
    pub fir_filter: FirFilter,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            reset_on_drop: true,
            tuner_probe: AnyTunerProbe::default(),
            fir_filter: FirFilter::DEFAULT,
        }
    }
}

#[derive(Debug)]
pub struct Device {
    device_info: DeviceInfo,
    reset_on_drop: bool,

    /// The RTL2832U and tuner are in an `Option` so we can take them out and
    /// spawn a task to run the reset code if this struct is dropped.
    inner: MaybeInner,

    /// The frequency correction factor (in ppm) that was set on the
    /// RTL2832U.
    frequency_correction: i16,

    /// The crystal frequency (in Hz) that was set on the
    /// RTL2832U.
    rtl_crystal_frequency: f32,

    /// The crystal frequency (in Hz) used for the tuner.
    tuner_crystal_frequency: f32,

    sample_rate: f32,
}

impl Device {
    pub async fn from_rtl2832u(
        rtl2832u: Rtl2832u,
        device_info: DeviceInfo,
        options: Options,
    ) -> Result<Self, Error> {
        // todo: should we try to reset the device if initialization fails?

        let mut transaction = rtl2832u.begin_transaction().await;

        // initialize baseband
        transaction.initialize(&options.fir_filter).await?;

        let rtl_crystal_frequency = rtl2832u::DEFAULT_CRYSTAL_FREQUENCY as f32;

        // probe tuners

        let i2c_repeater_guard = transaction.enable_i2c_repeater().await?;

        let tuner = options
            .tuner_probe
            .try_open(&rtl2832u)
            .await?
            .ok_or(Error::NoTunerFound)?;

        i2c_repeater_guard.disable().await?;

        tracing::info!(tuner = tuner.name(), "found tuner");

        // todo: this is specifically for R828D and Blog v4 for testing

        // if **not** blog v4, set tuner_xtal = R828D_XTAL_FREQ, otherwise use rtl_xtal.
        // librtlsdr uses the corrected crystal frequency here
        let tuner_crystal_frequency = rtl_crystal_frequency;
        transaction.set_if_mode(IfMode::If).await?;
        transaction
            .set_if_frequency(
                r82xx::DEFAULT_IF_FREQUENCY as f32,
                rtl2832u::DEFAULT_CRYSTAL_FREQUENCY as f32,
            )
            .await?;
        transaction.enable_spectrum_inversion(true).await?;

        let sample_rate = transaction.get_sample_rate(tuner_crystal_frequency).await?;
        tracing::debug!(?sample_rate, "initial sample rate");

        drop(transaction);

        Ok(Self {
            device_info,
            reset_on_drop: options.reset_on_drop,
            inner: MaybeInner(Some(Inner { rtl2832u, tuner })),
            frequency_correction: 0,
            rtl_crystal_frequency,
            tuner_crystal_frequency,
            sample_rate,
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
        let mut inner = self.inner.expect_take();
        inner.reset().await?;
        Ok(())
    }

    #[inline(always)]
    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    /// todo: pub for testing only
    pub fn tuner(&mut self) -> &mut AnyTuner {
        &mut self.inner.expect_mut().tuner
    }

    /// todo: pub for testing only
    pub fn rtl2832u(&mut self) -> &Rtl2832u {
        &self.inner.expect_ref().rtl2832u
    }

    pub async fn reader(&self, buffer_size: usize) -> Result<Reader, Error> {
        let rtl2832u = &self.inner.expect_ref().rtl2832u;
        let mut transaction = rtl2832u.begin_transaction().await;

        transaction.start_epa().await?;

        let inner = rtl2832u.epa_reader(buffer_size)?;

        Ok(Reader {
            inner,
            rtl2832u: Some(rtl2832u.clone()),
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

            let rtl2832u = &self.inner.expect_ref().rtl2832u;
            let mut transaction = rtl2832u.begin_transaction().await;

            transaction
                .set_sample_frequency_correction(frequency_correction)
                .await?;

            self.frequency_correction = frequency_correction;
        }

        Ok(())
    }

    pub async fn set_sample_rate(&mut self, sample_rate: f32) -> Result<(), Error> {
        tracing::debug!(?sample_rate, "setting sample rate");

        let inner = self.inner.expect_mut();

        let mut transaction = inner.rtl2832u.begin_transaction().await;

        // librtlsdr sets the "exact" sample rate here. We think they basically convert
        // from the encoded value back to Hz. But they also do some bit-manipulation.
        {
            let i2c_repeater_guard = transaction.enable_i2c_repeater().await?;
            inner.tuner.set_bandwidth(sample_rate).await?;
            i2c_repeater_guard.disable().await?;
        }

        // todo: in `r820t_set_bw` this also sets the if_freq.
        // it also calls `rtlsdr_set_center_freq`, which basically just calls
        // `rtlsdr_set_if_freq` (direct sampling) or `tuner->set_if_freq`

        let actual_sample_rate = transaction
            .set_sample_rate(sample_rate as f32, self.rtl_crystal_frequency)
            .await?;

        tracing::debug!(?actual_sample_rate);

        self.sample_rate = actual_sample_rate;

        Ok(())
    }

    pub fn sample_rate(&self) -> f32 {
        self.sample_rate
    }

    pub async fn set_center_frequency(&mut self, center_frequency: f32) -> Result<(), Error> {
        tracing::debug!(?center_frequency, "setting center frequency");

        let inner = self.inner.expect_mut();

        let mut transaction = inner.rtl2832u.begin_transaction().await;

        // librtlsdr sets the "exact" sample rate here. We think they basically convert
        // from the encoded value back to Hz. But they also do some bit-manipulation.
        {
            let i2c_repeater_guard = transaction.enable_i2c_repeater().await?;
            //inner.tuner.set_
            i2c_repeater_guard.disable().await?;
        }

        //transaction.set_if_mode(if_mode).await?;

        //transaction.set_if_frequency(if_frequency).await?;

        todo!();
    }
}

#[derive(derive_more::Debug)]
struct MaybeInner(Option<Inner>);

impl MaybeInner {
    #[inline(always)]
    fn expect_ref(&self) -> &Inner {
        self.0.as_ref().expect("device lost")
    }

    #[inline(always)]
    fn expect_mut(&mut self) -> &mut Inner {
        self.0.as_mut().expect("device lost")
    }

    #[inline(always)]
    fn expect_take(&mut self) -> Inner {
        self.0.take().expect("device lost")
    }
}

#[derive(Debug)]
struct Inner {
    rtl2832u: Rtl2832u,
    tuner: AnyTuner,
}

impl Inner {
    async fn reset(&mut self) -> Result<(), Error> {
        let mut transaction = self.rtl2832u.begin_transaction().await;

        let i2c_repeater_guard = transaction.enable_i2c_repeater().await?;
        self.tuner.shutdown().await?;
        i2c_repeater_guard.disable().await?;

        transaction.reset(Default::default()).await?;

        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        tracing::debug!(reset_on_drop = ?self.reset_on_drop, inner_present = self.inner.0.is_some(), "device dropped");

        if self.reset_on_drop
            && let Some(mut inner) = self.inner.0.take()
        {
            tokio::spawn(async move {
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
/// Otherwise you can also disable this behavior via the [`disam_stop_on_drop`]
/// method.
#[derive(Debug)]
pub struct Reader {
    inner: EpaReader,
    rtl2832u: Option<Rtl2832u>,
    stop_on_drop: bool,
}

impl Reader {
    /// Sets the number of concurrent USB transfers.
    ///
    /// Refer to the documentation of [`nusb::io::EndpointRead`] for more
    /// information.
    pub fn set_num_transfers(&mut self, num_transfers: usize) {
        self.inner.set_num_transfers(num_transfers);
    }

    /// Disable the default drop behavior.
    ///
    /// By default EPA is reset and stalled on drop, and is likely what you
    /// want. This lets you override this behavior.
    pub fn disarm_stop_on_drop(&mut self) {
        self.stop_on_drop = false;
    }

    pub async fn close(mut self) -> Result<(), Error> {
        let rtl2832u = self
            .rtl2832u
            .take()
            .expect("rtl2832u is missing. was this Reader dropped before?");

        let mut transaction = rtl2832u.begin_transaction().await;

        transaction.stop_epa().await?;
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
        Pin::new(&mut self.get_mut().inner).poll_read(cx, buf)
    }
}

impl AsyncBufRead for Reader {
    #[inline(always)]
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<&[u8]>> {
        Pin::new(&mut self.get_mut().inner).poll_fill_buf(cx)
    }

    #[inline(always)]
    fn consume(self: Pin<&mut Self>, amt: usize) {
        Pin::new(&mut self.get_mut().inner).consume(amt);
    }
}

impl Drop for Reader {
    fn drop(&mut self) {
        if self.stop_on_drop
            && let Some(rtl2832u) = self.rtl2832u.take()
        {
            tokio::spawn(async move {
                let mut transaction = rtl2832u.begin_transaction().await;
                if let Err(error) = transaction.stop_epa().await {
                    tracing::error!(%error, "Error stopping EPA while dropping");
                }
            });
        }
    }
}

/// Applies `correction` (in PPM) to `frequency` (in Hz).
#[inline(always)]
pub fn apply_frequency_correction(frequency: u32, correction: i16) -> u32 {
    // todo: check for overflow?
    (frequency as f32 * (1.0 + correction as f32 / 1.0e6)) as u32
}
