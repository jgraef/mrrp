use crate::{
    config::Config,
    directories::Directories,
    ui::waterfall::Waterfall,
};

#[derive(Debug)]
pub struct App {
    directories: Directories,
    config: Config,
}

impl App {
    pub fn new(directories: Directories, config: Config) -> Self {
        Self {
            directories,
            config,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show_inside(ui, |ui| {
            ui.heading("Hello World");
            ui.add(Waterfall::new());
        });
    }
}
