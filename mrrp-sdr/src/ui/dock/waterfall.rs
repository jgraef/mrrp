use egui::Color32;
use mrrp_widgets::waterfall::{
    WaterfallState,
    WaterfallStyle,
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

// todo: merge waterfall and spectrum dock. this allow unified controls for
// min/max_db / start/end_frequency, colors, etc.

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

        let center = 7000000.0;
        let width = 2400000.0;
        ui.add(
            WaterfallView::new(&self.state.view_state)
                .frequency_range(center - 0.5 * width, center + 0.5 * width)
                .style(WaterfallStyle {
                    background_color: Color32::TRANSPARENT,
                    foreground_color1: Color32::BLACK,
                    foreground_color2: Color32::MAGENTA,
                }),
        );
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
