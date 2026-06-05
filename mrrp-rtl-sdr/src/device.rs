use crate::{
    Error,
    enumerate::DeviceInfo,
    rtl2832u::{
        DEFAULT_CRYSTAL_FREQUENCY,
        IfMode,
        Reader,
        Rtl2832u,
        filter::FirFilter,
    },
    tuner::{
        AnyTuner,
        AnyTunerProbe,
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
    inner: Option<Inner>,

    /// The frequency correction factor (in ppm) that was set on the
    /// RTL2832U.
    frequency_correction: i16,
    // /// The crystal frequency (in Hz) that was set on the
    // /// RTL2832U.
    // rtl_crystal_frequency: u32,
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
        let tuner_probe = options.tuner_probe;
        let tuner = rtl2832u
            .with_i2c_repeater::<_, _, Error>(async move |mut rtl2832u| {
                tuner_probe
                    .try_open(&mut rtl2832u)
                    .await
                    .map_err(Into::into)
            })
            .await?
            .ok_or(Error::NoTunerFound)?;

        // todo: this is specifically for R828D and Blog v4 for testing

        // if blog v4, set tuner_xtal = R828D_XTAL_FREQ, otherwise use rtl_xtal
        // librtlsdr uses the corrected crystal frequency
        rtl2832u.set_if_mode(IfMode::If).await?;
        rtl2832u
            .set_if_frequency(r82xx::DEFAULT_IF_FREQUENCY, DEFAULT_CRYSTAL_FREQUENCY)
            .await?;
        rtl2832u.enable_spectrum_inversion(true).await?;

        Ok(Self {
            device_info,
            reset_on_drop: options.reset_on_drop,
            inner: Some(Inner { rtl2832u, tuner }),
            frequency_correction: 0,
            // rtl_crystal_frequency: DEFAULT_CRYSTAL_FREQUENCY,
        })
    }

    pub async fn close(mut self) -> Result<(), Error> {
        tracing::debug!("closing device");
        let mut inner = self.inner.take().expect("device lost");
        inner.reset().await?;
        Ok(())
    }

    #[inline(always)]
    pub fn device_info(&self) -> &DeviceInfo {
        &self.device_info
    }

    #[inline(always)]
    fn expect_inner_mut(&mut self) -> &mut Inner {
        self.inner.as_mut().expect("device lost")
    }

    /// todo: pub for testing only
    pub fn tuner(&mut self) -> &mut AnyTuner {
        &mut self.expect_inner_mut().tuner
    }

    /// todo: pub for testing only
    pub fn rtl2832(&mut self) -> &mut Rtl2832u {
        &mut self.expect_inner_mut().rtl2832u
    }

    pub async fn reader(&mut self, buffer_size: usize) -> Result<Reader, Error> {
        // todo: we need to wrap the rtl2832u::Reader and add a Drop impl that stops the
        // data stream.

        let rtl2832u = &mut self.expect_inner_mut().rtl2832u;
        rtl2832u.start_data_stream().await?;
        Ok(rtl2832u.reader(buffer_size)?)
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

            self.rtl2832()
                .set_sample_frequency_correction(frequency_correction)
                .await?;

            self.frequency_correction = frequency_correction;
        }

        Ok(())
    }
}

#[derive(Debug)]
struct Inner {
    rtl2832u: Rtl2832u,
    tuner: AnyTuner,
}

impl Inner {
    async fn reset(&mut self) -> Result<(), Error> {
        // todo: reset gpio

        self.rtl2832u.reset().await?;

        Ok(())
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        tracing::debug!(reset_on_drop = ?self.reset_on_drop, inner_present = self.inner.is_some(), "device dropped");

        if self.reset_on_drop
            && let Some(mut inner) = self.inner.take()
        {
            tokio::spawn(async move {
                if let Err(error) = inner.reset().await {
                    tracing::error!(%error, "Error resetting RTL2832U while dropping");
                }
            });
        }
    }
}

/// Applies `correction` (in PPM) to `frequency` (in Hz).
#[inline]
pub fn apply_frequency_correction(frequency: u32, correction: i16) -> u32 {
    // todo: check for overflow?
    (frequency as f32 * (1.0 + correction as f32 / 1.0e6)) as u32
}
