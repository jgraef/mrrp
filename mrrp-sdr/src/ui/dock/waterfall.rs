use mrrp_widgets::waterfall::{
    WaterfallState,
    WaterfallView,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::sdr::{
    SpectrumSinkHandle,
    ensure_spectrum_sink_is_linked,
};

#[derive(Debug)]
pub struct WaterfallDock<'a> {
    state: &'a mut WaterfallDockState,
}

impl<'a> WaterfallDock<'a> {
    pub fn new(state: &'a mut WaterfallDockState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) {
        ensure_spectrum_sink_is_linked(
            ui.ctx(),
            &self.state.view_state,
            &mut self.state.sdr_link_handle,
        );

        ui.add(WaterfallView::new(&self.state.view_state));
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WaterfallDockState {
    /// Holds state for the WaterfallView.
    #[serde(skip, default)]
    view_state: WaterfallState,

    #[serde(skip, default)]
    sdr_link_handle: Option<SpectrumSinkHandle>,
}
