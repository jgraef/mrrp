use mrrp_widgets::spectrum::{
    SpectrumState,
    SpectrumView,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::sdr::{
    GetSdrHandle,
    SpectrumSinkHandle,
    sink::SpectrumSink,
};

#[derive(Debug)]
pub struct SpectrumDock<'a> {
    state: &'a mut SpectrumDockState,
}

impl<'a> SpectrumDock<'a> {
    pub fn new(state: &'a mut SpectrumDockState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) {
        ensure_sink_is_linked(
            ui.ctx(),
            &self.state.view_state,
            &mut self.state.sdr_link_handle,
        );

        ui.add(SpectrumView::new(&self.state.view_state));
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpectrumDockState {
    /// Holds GPU resources (pipeline, buffers, etc.)
    #[serde(skip, default)]
    view_state: SpectrumState,

    #[serde(skip, default)]
    sdr_link_handle: Option<SpectrumSinkHandle>,
}

fn ensure_sink_is_linked<S>(ctx: &egui::Context, sink: &S, handle: &mut Option<SpectrumSinkHandle>)
where
    S: SpectrumSink + Send + Clone + 'static,
{
    if handle.is_none() {
        let sdr = ctx.expect_sdr_handle();
        *handle = Some(sdr.add_spectrum_sink(sink.clone()));
    }
}
