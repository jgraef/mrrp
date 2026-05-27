use mrrp_widgets::waterfall::{
    WaterfallState,
    WaterfallView,
};
use serde::{
    Deserialize,
    Serialize,
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
        ui.add(WaterfallView::new(&self.state.view_state));
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WaterfallDockState {
    /// Holds state for the WaterfallView.
    #[serde(skip, default)]
    view_state: WaterfallState,
}
