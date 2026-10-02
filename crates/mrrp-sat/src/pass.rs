use crate::satellite::{
    ReferenceState,
    RelativeState,
    SatelliteState,
};

pub struct PassDetector {
    min_elevation: f64,
    reference_state: ReferenceState,
    previous_state: Option<PassState>,
    ongoing_pass_state: Option<OngoingPassState>,
}

impl PassDetector {
    pub fn new(min_elevation: f64, reference_state: ReferenceState) -> Self {
        Self {
            min_elevation,
            reference_state,
            previous_state: None,
            ongoing_pass_state: None,
        }
    }

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
    Start(PassState),
    Ongoing(OngoingPassState),
    End(CompletePass),
}

#[derive(Clone, Copy, Debug)]
pub struct PassState {
    pub state: SatelliteState,
    pub relative: RelativeState,
}

#[derive(Clone, Copy, Debug)]
pub struct CompletePass {
    pub start: PassState,
    pub end: PassState,
    pub max_elevation: PassState,
    pub closest_approach: PassState,
}

#[derive(Clone, Copy, Debug)]
pub struct OngoingPassState {
    pub start: PassState,
    pub max_elevation: PassState,
    pub closest_approach: PassState,
}

impl OngoingPassState {
    fn new(start: PassState) -> Self {
        Self {
            start,
            max_elevation: start,
            closest_approach: start,
        }
    }
}
