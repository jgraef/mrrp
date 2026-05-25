use crate::ui::{
    app::AppState,
    radio::RadioUiState,
};

#[derive(Debug)]
pub struct MainMenu<'a> {
    pub radio_state: &'a mut RadioUiState,
    pub app_state: &'a mut AppState,
}

impl<'a> egui::Widget for MainMenu<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        egui::MenuBar::new()
            .ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if self.radio_state.is_connected() {
                        if ui.button("Stop Capture").clicked() {
                            // todo: stop capture
                        }
                    }
                    else {
                        if ui.button("Start Capture").clicked() {
                            // todo: show radio selection dialog
                        }
                    }

                    if ui.button("Open Capture").clicked() {
                        // todo: open recorded capture
                    }

                    if ui.button("Configure Radios").clicked() {
                        // todo: show radio configuration dialog
                    }

                    if ui.button("Exit").clicked() {
                        ui.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                });

                ui.menu_button("View", |ui| {
                    if ui.button("Command Palette").clicked() {

                        // todo
                    }

                    ui.checkbox(
                        &mut self.app_state.show_baseband_spectrum,
                        "Baseband Spectrum",
                    );
                    ui.checkbox(
                        &mut self.app_state.show_baseband_waterfall,
                        "Baseband Waterfall",
                    );
                    ui.checkbox(&mut self.app_state.show_channels, "Channels");
                    ui.checkbox(&mut self.app_state.show_bookmarks, "Bookmarks");
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About mrrp-sdr").clicked() {
                        self.app_state.show_about_window = true;
                    }

                    if ui.button("File Bug Report").clicked() {
                        let url = format!("{}issues", std::env!("CARGO_PKG_REPOSITORY"));
                        ui.open_url(egui::OpenUrl::new_tab(url));
                    }
                })
            })
            .response
    }
}
