use const_format::formatcp;
use eframe::Storage;
use egui::ScrollArea;
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    cli::UiCommand,
    config::Config,
    directories::Directories,
    ui::{
        about_window::AboutWindow,
        menu::MainMenu,
        radio::{
            RadioConfigWindow,
            RadioUi,
            RadioUiState,
        },
        spectrum::{
            Spectrum,
            SpectrumBuffer,
        },
        waterfall::Waterfall,
    },
};

#[derive(Debug)]
pub struct App {
    directories: Directories,
    config: Config,

    radio_state: RadioUiState,

    app_state: AppState,

    spectrum_buffer: SpectrumBuffer,
}

impl App {
    pub fn new(
        directories: Directories,
        config: Config,
        command: UiCommand,
        storage: &dyn Storage,
    ) -> Self {
        let radio_state = RadioUiState::new(&config, &command);

        let app_state = if command.reset_app_state {
            AppState::default()
        }
        else {
            AppState::load(storage)
        };

        Self {
            directories,
            config,
            radio_state,
            app_state,
            spectrum_buffer: Default::default(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("menu_panel").show_inside(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.add(MainMenu {
                    radio_state: &mut self.radio_state,
                    app_state: &mut self.app_state,
                })
            });
        });

        egui::Panel::top("toolbar_panel").show_inside(ui, |ui| {
            ui.heading("Top Panel");

            // todo
        });

        egui::Panel::left("left_panel")
            .resizable(true)
            .show_inside(ui, |ui| {
                ScrollArea::both().show(ui, |ui| {
                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        ui.make_persistent_id("radio_collapsed"),
                        true,
                    )
                    .show_header(ui, |ui| {
                        ui.heading(formatcp!("{} Radio", egui_phosphor::regular::RADIO));
                    })
                    .body_unindented(|ui| ui.add(RadioUi::new(&mut self.radio_state)));

                    ui.add_space(10.0);

                    egui::collapsing_header::CollapsingState::load_with_default_open(
                        ui.ctx(),
                        ui.make_persistent_id("demod_collapsed"),
                        true,
                    )
                    .show_header(ui, |ui| {
                        ui.heading(formatcp!(
                            "{} Demodulation",
                            egui_phosphor::regular::WAVE_SINE
                        ));
                    })
                    .body_unindented(|ui| {
                        ui.label("TODO");
                    });

                    ui.take_available_space();
                });
            });

        egui::Panel::top("spectrum")
            .min_size(100.0)
            .resizable(true)
            .show_inside(ui, |ui| {
                ui.add(Spectrum::new(&self.spectrum_buffer));
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.add(Waterfall::new());
        });

        RadioConfigWindow::new(&mut self.radio_state).show(ui.ctx());

        AboutWindow::new(&mut self.app_state).show(ui.ctx());
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app_state.save(storage);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    #[serde(skip, default)]
    pub persist: bool,

    #[serde(skip, default)]
    pub show_about_window: bool,

    pub show_baseband_spectrum: bool,
    pub show_baseband_waterfall: bool,
    pub show_channels: bool,
    pub show_bookmarks: bool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            persist: true,
            show_about_window: false,
            show_baseband_spectrum: true,
            show_baseband_waterfall: true,
            show_channels: true,
            show_bookmarks: false,
        }
    }
}

impl AppState {
    const KEY: &str = "app_state";

    fn load(storage: &dyn eframe::Storage) -> Self {
        tracing::debug!("loading app state");

        if let Some(value) = storage.get_string(Self::KEY) {
            match serde_json::from_str::<Self>(&value) {
                Ok(mut state) => {
                    state.persist = true;
                    state
                }
                Err(error) => {
                    tracing::error!(%error, "Failed to load app state. Using default state, but will not persist it. Use --reset-state to reset.");
                    let mut state = Self::default();
                    state.persist = false;
                    state
                }
            }
        }
        else {
            tracing::debug!("No app state present. Using default");
            let mut state = Self::default();
            state.persist = true;
            state
        }
    }

    fn save(&self, storage: &mut dyn eframe::Storage) {
        tracing::debug!("saving app state");
        let value = serde_json::to_string(self).expect("main app state serialization");
        storage.set_string(Self::KEY, value);
    }
}
