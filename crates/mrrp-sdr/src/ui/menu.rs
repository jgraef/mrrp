use egui::Frame;

use crate::{
    ui::{
        dock::DockAddTabMenu,
        state::{
            AppState,
            CommandBuffer,
        },
    },
    util::github_urls::GithubUrls,
};

#[derive(Debug)]
pub struct MainMenuPanel<'a> {
    app_state: &'a mut AppState,
    command_buffer: &'a mut CommandBuffer,
}

impl<'a> MainMenuPanel<'a> {
    pub fn new(app_state: &'a mut AppState, command_buffer: &'a mut CommandBuffer) -> Self {
        Self {
            app_state,
            command_buffer,
        }
    }
}

impl<'a> egui::Widget for MainMenuPanel<'a> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        egui::Panel::top("menu_panel")
            .frame(Frame::side_top_panel(ui.style()).fill(ui.style().visuals.extreme_bg_color))
            .show_inside(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.add(MainMenu::new(&mut self.app_state, &mut self.command_buffer))
                });
            })
            .response
    }
}

#[derive(Debug)]
pub struct MainMenu<'a> {
    app_state: &'a mut AppState,
    command_buffer: &'a mut CommandBuffer,
}

impl<'a> MainMenu<'a> {
    pub fn new(app_state: &'a mut AppState, command_buffer: &'a mut CommandBuffer) -> Self {
        Self {
            app_state,
            command_buffer,
        }
    }
}

impl<'a> egui::Widget for MainMenu<'a> {
    fn ui(mut self, ui: &mut egui::Ui) -> egui::Response {
        egui::MenuBar::new()
            .ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    //if self.radio_state.is_connected() {
                    if false {
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

                    ui.menu_button("Add Dock", |ui| {
                        ui.add(DockAddTabMenu::new(
                            &mut self.app_state,
                            &mut self.command_buffer,
                        ));
                    });

                    ui.menu_button("Zoom", |ui| {
                        if ui.button("Fill view with signal").clicked() {
                            // todo
                        }

                        // come up with more
                    });
                });

                ui.menu_button("Help", |ui| {
                    if ui.button("About mrrp-sdr").clicked() {
                        self.app_state.show_about_window = true;
                    }

                    if ui.button("File Bug Report").clicked() {
                        ui.open_url(egui::OpenUrl::new_tab(GithubUrls::PACKAGE.issues()));
                    }

                    if ui.button("Debug").clicked() {
                        self.app_state.show_debug_window = true;
                    }
                })
            })
            .response
    }
}
