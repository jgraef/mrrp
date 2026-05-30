use std::ops::RangeInclusive;

use egui::{
    Checkbox,
    Color32,
    Frame,
    Slider,
    containers::menu::menu_style,
};
use mrrp_widgets::{
    colormap::ColorMap,
    spectrum::{
        SpectrumState,
        SpectrumView,
    },
    waterfall::{
        WaterfallState,
        WaterfallStyle,
        WaterfallView,
    },
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::sdr::{
    GetSdrHandle,
    SpectrumSinkHandle,
    sink::{
        RepaintOnPush,
        SpectrumSink,
    },
};

#[derive(Debug)]
pub struct SpectrumWaterfallDockView<'a> {
    state: &'a mut SpectrumWaterfallDockState,
}

impl<'a> SpectrumWaterfallDockView<'a> {
    pub fn new(state: &'a mut SpectrumWaterfallDockState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) {
        self.state.ensure_linked(ui.ctx());

        let id = ui.id().with("spectrum_waterfall_dock");

        let height = ui.available_height();

        egui::Panel::right(id.with("right_panel"))
            .frame(Frame::NONE)
            .resizable(false)
            .default_size(0.0)
            .size_range(0.0..=f32::INFINITY)
            .show_inside(ui, |ui| {
                ui.add(
                    Slider::new(&mut self.state.shared.max_db, 0.0..=100.0)
                        .vertical()
                        .show_value(false),
                )
                .on_hover_text("dB scale");

                ui.add_space(20.0);

                ui.add(
                    Slider::new(&mut self.state.shared.min_db, -100.0..=0.0)
                        .vertical()
                        .show_value(false),
                )
                .on_hover_text("dB offset");
            });

        egui::CentralPanel::default()
            .frame(Frame::NONE)
            .show_inside(ui, |ui| {
                match (&mut self.state.spectrum, &mut self.state.waterfall) {
                    (None, None) => unreachable!(),
                    (None, Some(waterfall)) => {
                        waterfall.show(ui, &self.state.shared);
                    }
                    (Some(spectrum), None) => {
                        spectrum.show(ui, &self.state.shared);
                    }
                    (Some(spectrum), Some(waterfall)) => {
                        egui::Panel::top(id.with("top_panel"))
                            .frame(Frame::NONE)
                            .default_size(0.2 * height)
                            .resizable(true)
                            .show_inside(ui, |ui| {
                                spectrum.show(ui, &self.state.shared);
                            });

                        egui::CentralPanel::default()
                            .frame(Frame::NONE)
                            .show_inside(ui, |ui| {
                                waterfall.show(ui, &self.state.shared);
                            });
                    }
                }
            });
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SpectrumWaterfallDockState {
    // these store the state of the spectrum view or waterfall view - whatever is visible. note
    // that at least one must be visible (just because it wouldn't make sense to have an empty
    // dock)
    spectrum: Option<PanelState<SpectrumState>>,
    waterfall: Option<PanelState<WaterfallState>>,

    shared: SharedState,
}

impl SpectrumWaterfallDockState {
    fn empty() -> Self {
        Self {
            spectrum: None,
            waterfall: None,
            shared: SharedState::default(),
        }
    }

    fn ensure_linked(&mut self, ctx: &egui::Context) {
        if let Some(spectrum) = &mut self.spectrum {
            spectrum.ensure_linked(ctx);
        }

        if let Some(waterfall) = &mut self.waterfall {
            waterfall.ensure_linked(ctx);
        }
    }

    pub fn add_spectrum(&mut self) {
        self.spectrum = Some(PanelState::new(SpectrumState::default()));
    }

    pub fn add_waterfall(&mut self) {
        self.waterfall = Some(PanelState::new(WaterfallState::default()));
    }

    pub fn remove_spectrum(&mut self) {
        if self.can_remove_spectrum() {
            self.spectrum = None;
        }
    }

    pub fn remove_waterfall(&mut self) {
        if self.can_remove_waterfall() {
            self.waterfall = None;
        }
    }

    pub fn can_remove_spectrum(&self) -> bool {
        self.has_waterfall()
    }

    pub fn can_remove_waterfall(&self) -> bool {
        self.has_spectrum()
    }

    pub fn has_spectrum(&self) -> bool {
        self.spectrum.is_some()
    }

    pub fn has_waterfall(&self) -> bool {
        self.waterfall.is_some()
    }

    pub fn spectrum() -> Self {
        let mut state = Self::empty();
        state.add_spectrum();
        state
    }

    pub fn waterfall() -> Self {
        let mut state = Self::empty();
        state.add_waterfall();
        state
    }

    pub fn both() -> Self {
        let mut state = Self::empty();
        state.add_spectrum();
        state.add_waterfall();
        state
    }

    pub fn title(&self) -> egui::WidgetText {
        match (self.spectrum.is_some(), self.waterfall.is_some()) {
            (false, false) => unreachable!(),
            (false, true) => "Waterfall",
            (true, false) => "Spectrum",
            (true, true) => "Spectrum & Waterfall",
        }
        .into()
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PanelState<S> {
    // handle to registration in SDR runtime for this sink.
    //
    // when this is dropped (i.e. when the spectrum or waterfall is removed) it will automatically
    // unregister.
    #[serde(skip, default)]
    sdr_link_handle: Option<SpectrumSinkHandle>,

    /// Holds GPU resources (pipeline, buffers, etc.)
    #[serde(skip, default)]
    state: S,
}

impl<S> PanelState<S> {
    fn new(state: S) -> Self {
        Self {
            sdr_link_handle: None,
            state,
        }
    }
}

impl PanelState<SpectrumState> {
    fn show(&mut self, ui: &mut egui::Ui, shared: &SharedState) {
        ui.add(SpectrumView::new(&self.state).db_range(shared.db_range()));
    }
}

impl PanelState<WaterfallState> {
    fn show(&mut self, ui: &mut egui::Ui, shared: &SharedState) {
        ui.add(
            WaterfallView::new(&self.state)
                .frequency_range(shared.start_frequency..=shared.end_frequency)
                .db_range(shared.db_range())
                .style(WaterfallStyle {
                    background_color: Color32::TRANSPARENT,
                    color_map: ColorMap::default(),
                }),
        );
    }
}

impl<S> PanelState<S>
where
    S: SpectrumSink + Send + Clone + 'static,
{
    fn ensure_linked(&mut self, ctx: &egui::Context) {
        if self.sdr_link_handle.is_none() {
            let sdr = ctx.expect_sdr_handle();
            self.sdr_link_handle =
                Some(sdr.add_spectrum_sink(RepaintOnPush::new(self.state.clone(), ctx.clone())));
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct SharedState {
    min_db: f32,
    max_db: f32,
    start_frequency: f32,
    end_frequency: f32,
}

impl Default for SharedState {
    fn default() -> Self {
        let center = 7000000.0;
        let width = 2400000.0;

        Self {
            min_db: -100.0,
            max_db: 100.0,
            start_frequency: center - 0.5 * width,
            end_frequency: center + 0.5 * width,
        }
    }
}

impl SharedState {
    fn db_range(&self) -> RangeInclusive<f32> {
        self.min_db..=(self.max_db + self.min_db)
    }
}

#[derive(Debug)]
pub struct SpectrumWaterfallDockContextMenu<'a> {
    state: &'a mut SpectrumWaterfallDockState,
}

impl<'a> SpectrumWaterfallDockContextMenu<'a> {
    pub fn new(state: &'a mut SpectrumWaterfallDockState) -> Self {
        Self { state }
    }

    pub fn show(mut self, ui: &mut egui::Ui) {
        ui.scope(|ui| {
            // apply menu style, so this always looks like a menu
            menu_style(ui.style_mut());

            #[inline(always)]
            fn show_checkbox(
                ui: &mut egui::Ui,
                state: &mut SpectrumWaterfallDockState,
                label: &str,
                is_active: impl FnOnce(&mut SpectrumWaterfallDockState) -> bool,
                can_disable: impl FnOnce(&mut SpectrumWaterfallDockState) -> bool,
                enable: impl FnOnce(&mut SpectrumWaterfallDockState),
                disable: impl FnOnce(&mut SpectrumWaterfallDockState),
            ) {
                let is_active = is_active(state);
                let can_disable = can_disable(state);

                let mut cb_state = is_active;

                ui.add_enabled(
                    !is_active || can_disable,
                    Checkbox::new(&mut cb_state, label),
                );

                if is_active && !cb_state && can_disable {
                    disable(state);
                }
                else if !is_active && cb_state {
                    enable(state);
                }
            }

            show_checkbox(
                ui,
                &mut self.state,
                "Spectrum",
                |state| state.has_spectrum(),
                |state| state.can_remove_spectrum(),
                |state| state.add_spectrum(),
                |state| state.remove_spectrum(),
            );

            show_checkbox(
                ui,
                &mut self.state,
                "Waterfall",
                |state| state.has_waterfall(),
                |state| state.can_remove_waterfall(),
                |state| state.add_waterfall(),
                |state| state.remove_waterfall(),
            );
        });
    }
}
