use std::time::Duration;

use chrono::{
    DateTime,
    Utc,
};
use mrrp_core::{
    sample::Complex,
    signal::GetSampleRate,
};
use mrrp_sat::satellite::{
    OrbitPropagationCache,
    ReferenceState,
    Satellite,
    SatelliteState,
};

use crate::signal::{
    ComplexSinusoid,
    Scanner,
    SignalGenerator,
};

/// Corrects doppler shift from satellites.
///
/// The easiest way to apply this to a signal is by using
/// [`scan_in_place_with`](AsyncReadSamples::scan_in_place_with). But since the
/// corrrection is done by frequency shift, this also implements
/// `SignalGenerator`. that generates the sinusoid with the right frequency.
///
/// # TODO
///
/// At the moment this works only for time-limited signals. This is because all
/// satellite states are calculated in the constructor so that we don't need to
/// keep a reference to the satellite data. For time-infinite signals it
/// would also make sense to pass in a data source so that the satellite data
/// can be updated.
#[derive(Clone, Debug)]
pub struct DopplerCorrection {
    satellite_states: Vec<SatelliteState>,
    reference_state: ReferenceState,
    nominal_frequency: f32,
    sinusoid: ComplexSinusoid,
    start_time: DateTime<Utc>,
    sample_index: usize,
    state_index: usize,
}

impl DopplerCorrection {
    pub fn from_satellite(
        satellite: &Satellite,
        reference_state: ReferenceState,
        nominal_frequency: f32,
        start_time: DateTime<Utc>,
        predict_interval: Duration,
        sample_rate: f32,
        duration: Duration,
    ) -> Result<Self, mrrp_sat::Error> {
        tracing::debug!(sat=satellite.name(), tle=?satellite.tle(), nominal_frequency, "Correcting doppler");

        let num_intervals =
            (duration.as_secs_f32() / predict_interval.as_secs_f32()).ceil() as usize;

        // calculate timing information
        let num_samples_per_interval =
            (predict_interval.as_secs_f32() * sample_rate).floor() as usize;
        let effective_predict_interval = num_samples_per_interval as f32 / sample_rate;

        tracing::debug!(
            ?num_intervals,
            ?num_samples_per_interval,
            ?effective_predict_interval
        );

        // calculate exact timestamps for which we want satellite states
        let times = (0..num_intervals)
            .map(|i| start_time + Duration::from_secs_f32(effective_predict_interval * i as f32))
            .collect::<Vec<_>>();

        // predict satellite states for the time spanned by the source
        let mut orbit_propagation_cache = OrbitPropagationCache::default();
        let satellite_states = satellite.predict_state(&times, &mut orbit_propagation_cache)?;

        Ok(Self::from_states(
            satellite_states,
            reference_state,
            nominal_frequency,
            sample_rate,
        ))
    }

    pub fn from_states(
        satellite_states: Vec<SatelliteState>,
        reference_state: ReferenceState,
        nominal_frequency: f32,
        sample_rate: f32,
    ) -> Self {
        // note: we initialize the frequency to 1 Hz, but it will be updated
        // before generating the first sample
        let sinusoid = ComplexSinusoid::new(1.0, sample_rate);

        let start_time = satellite_states
            .get(0)
            .unwrap_or_else(|| panic!("satellite_states must not be empty."))
            .time();

        Self {
            satellite_states,
            reference_state,
            nominal_frequency,
            sinusoid,
            start_time,
            sample_index: 0,
            state_index: 0,
        }
    }
}

impl Scanner<Complex<f32>> for DopplerCorrection {
    type Output = Complex<f32>;

    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        self.next() * sample
    }
}

impl SignalGenerator for DopplerCorrection {
    type Sample = Complex<f32>;

    fn next(&mut self) -> Self::Sample {
        // or should we just calculate the delta time per sample ahead of time
        // and use that? our concern was that this would accumulate error.
        let time = self.start_time
            + Duration::from_secs_f32(self.sample_index as f32 / self.sinusoid.sample_rate());

        // current state we're at
        let mut current_state = &self.satellite_states[self.state_index];

        // check if we moved on to the next state
        if let Some(next_state) = self.satellite_states.get(self.state_index + 1) {
            if time >= next_state.time() {
                self.state_index += 1;
                current_state = next_state;
            }
        }

        let relative_state = current_state.relative(&self.reference_state);
        let doppler_shift = relative_state.doppler_shift(self.nominal_frequency as f64) as f32;

        self.sample_index += 1;

        self.sinusoid.set_frequency(-doppler_shift);
        self.sinusoid.next()
    }
}

impl GetSampleRate for DopplerCorrection {
    fn sample_rate(&self) -> f32 {
        self.sinusoid.sample_rate()
    }
}
