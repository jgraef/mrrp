use const_format::formatcp;
use egui::ScrollArea;

use crate::{
    cli::UiCommand,
    config::Config,
    directories::Directories,
    ui::{
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

    spectrum_buffer: SpectrumBuffer,
}

impl App {
    pub fn new(directories: Directories, config: Config, command: UiCommand) -> Self {
        let radio_state = RadioUiState::new(&config, &command);

        Self {
            directories,
            config,
            radio_state,
            spectrum_buffer: Default::default(),
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top_panel").show_inside(ui, |ui| {
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
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }
}
