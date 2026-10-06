use std::time::Duration;

use chrono::{
    DateTime,
    TimeDelta,
    Utc,
};

use crate::{
    Error,
    Geodetic,
    satellite::{
        OrbitPropagationCache,
        ReferenceState,
        RelativeState,
        Satellite,
        SatelliteState,
    },
};

/// Detects when a satellite passes overhead.
///
/// This is a bit more low-level than [`PassEvents`], i.e. it doesn't calculate
/// the [`SatelliteState`]s itself, but you need to pass them in. It can be
/// useful if you just want to use the state machine for pass detection, but
/// nothing else.
#[derive(Clone, Debug)]
pub struct PassDetector {
    min_elevation: f64,
    reference_state: ReferenceState,
    previous_state: Option<PassState>,
    ongoing_pass_state: Option<OngoingPassState>,
}

impl PassDetector {
    #[inline]
    pub fn new(min_elevation: f64, reference_state: ReferenceState) -> Self {
        Self {
            min_elevation,
            reference_state,
            previous_state: None,
            ongoing_pass_state: None,
        }
    }

    /// Pushes a [`SatelliteState`] into the pass-detection state machine.
    ///
    /// Returns [`PassEvent`] when a pass starts, is currently ongoing, or ends.
    pub fn push(&mut self, state: SatelliteState) -> Option<PassEvent> {
        let relative = state.relative(&self.reference_state);
        let state = PassState { state, relative };
        let overhead = relative.elevation() >= self.min_elevation;

        let output = match (&mut self.ongoing_pass_state, overhead) {
            (None, true) => {
                // pass started

                self.ongoing_pass_state = Some(OngoingPassState::new(state));
                Some(PassEvent::Start(state))
            }
            (Some(ongoing_state), true) => {
                // pass ongoing

                if state.relative.elevation() > ongoing_state.max_elevation.relative.elevation() {
                    ongoing_state.max_elevation = state;
                }
                if state.relative.distance() < ongoing_state.closest_approach.relative.distance() {
                    ongoing_state.closest_approach = state;
                }

                Some(PassEvent::Ongoing(*ongoing_state))
            }
            (Some(ongoing_state), false) => {
                // pass end

                let ongoing_state = *ongoing_state;
                self.ongoing_pass_state = None;

                // this can't be None, as there must have been at least one call
                // to this method before, or otherwise self.pass_start would
                // have been None too.
                let end = self.previous_state.unwrap();

                Some(PassEvent::End(CompletePass {
                    start: ongoing_state.start,
                    end,
                    max_elevation: ongoing_state.max_elevation,
                    closest_approach: ongoing_state.closest_approach,
                }))
            }
            (None, false) => {
                // below horizon
                None
            }
        };

        self.previous_state = Some(state);

        output
    }
}

#[derive(Clone, Copy, Debug)]
pub enum PassEvent {
    /// A satellite pass just started.
    ///
    /// This contains the first satellite state that is above the minimum
    /// elevation.
    Start(PassState),

    /// A satellite pass is currently ongoing.
    ///
    /// Contains the start state and some states that are being kept track of
    /// (closest approach, max elevation).
    Ongoing(OngoingPassState),

    /// A satellite pass has ended.
    ///
    /// Contains start and end state and some other notable states.
    End(CompletePass),
}

#[derive(Clone, Copy, Debug)]
pub struct PassState {
    /// The satellite state independent of an observer
    pub state: SatelliteState,

    /// The satellite state relative to an observer.
    ///
    /// This contains distance and relative velocity to the satellite, which can
    /// be use to compute helpful information such as azimuth, elevation,
    /// free-space path loss and doppler shift.
    pub relative: RelativeState,
}

/// A complete satellite pass
#[derive(Clone, Copy, Debug)]
pub struct CompletePass {
    /// First state at which the satellite was above min elevation.
    pub start: PassState,

    /// Last state at which the satellite was above min elevation.
    pub end: PassState,

    /// The state with the max elevation.
    pub max_elevation: PassState,

    /// The state with the closest approach.
    pub closest_approach: PassState,
}

impl CompletePass {
    /// Duration of the complete satellite pass.
    #[inline]
    pub fn duration(&self) -> TimeDelta {
        self.end.state.time() - self.start.state.time()
    }
}

/// A ongoing satellite pass.
///
/// `max_elevation` and `closest_approach` only take into account all states
/// that have been seen yet.
#[derive(Clone, Copy, Debug)]
pub struct OngoingPassState {
    /// First state at which the satellite whas above min elevation.
    pub start: PassState,

    /// The state with the max elevation so far.
    pub max_elevation: PassState,

    /// The state with the closest approach so far.
    pub closest_approach: PassState,
}

impl OngoingPassState {
    #[inline]
    fn new(start: PassState) -> Self {
        Self {
            start,
            max_elevation: start,
            closest_approach: start,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PassEventsOptions {
    pub start_time: Option<DateTime<Utc>>,
    pub predict_interval: Duration,
    pub batch_size: usize,
    pub min_elevation: f64,
}

impl Default for PassEventsOptions {
    #[inline]
    fn default() -> Self {
        Self {
            start_time: None,
            predict_interval: Duration::from_secs(30),
            batch_size: 1024,
            min_elevation: 10.0f64.to_radians(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PassEvents<'a> {
    satellite: &'a Satellite,
    predict_interval: f32,
    batch_size: usize,
    time: DateTime<Utc>,
    times_buffer: Vec<DateTime<Utc>>,
    state_buffer: Vec<SatelliteState>,
    state_buffer_index: usize,
    orbit_propagation_cache: OrbitPropagationCache,
    pass_detector: PassDetector,
}

impl<'a> PassEvents<'a> {
    pub fn new(satellite: &'a Satellite, location: Geodetic, options: PassEventsOptions) -> Self {
        let time = options.start_time.unwrap_or_else(Utc::now);
        let pass_detector = PassDetector::new(
            options.min_elevation,
            ReferenceState::from_geodetic(location),
        );

        Self {
            satellite,
            predict_interval: options.predict_interval.as_secs_f32(),
            batch_size: options.batch_size,
            time,
            times_buffer: Vec::with_capacity(options.batch_size),
            state_buffer: Vec::with_capacity(options.batch_size),
            state_buffer_index: 0,
            orbit_propagation_cache: OrbitPropagationCache::default(),
            pass_detector,
        }
    }

    #[inline]
    pub fn set_cache(&mut self, cache: OrbitPropagationCache) {
        self.orbit_propagation_cache = cache;
    }

    #[inline]
    pub fn get_cache(&self) -> OrbitPropagationCache {
        self.orbit_propagation_cache.clone()
    }
}

impl<'a> Iterator for PassEvents<'a> {
    type Item = Result<PassEvent, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.state_buffer_index < self.state_buffer.len() {
                // states buffered, use them up first

                let state = self.state_buffer[self.state_buffer_index];
                self.state_buffer_index += 1;

                if let Some(event) = self.pass_detector.push(state) {
                    return Some(Ok(event));
                }
            }
            else {
                // no states buffered. calculate next batch

                // clear states buffer
                self.state_buffer.clear();
                self.state_buffer_index = 0;

                // fill times buffer with next batch of timestamps
                self.times_buffer.clear();
                for _ in 0..self.batch_size {
                    self.times_buffer.push(self.time);
                    self.time += Duration::from_secs_f32(self.predict_interval);
                }

                // predict satellite states
                if let Err(error) = self.satellite.predict_state_into(
                    &self.times_buffer,
                    &mut self.orbit_propagation_cache,
                    &mut self.state_buffer,
                ) {
                    return Some(Err(error.into()));
                }
            }
        }
    }
}
