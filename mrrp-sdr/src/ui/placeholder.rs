use egui::Widget;

#[derive(Clone, Copy, Debug)]
pub struct PlaceholderUi<'a> {
    label: &'a str,
}

impl<'a> PlaceholderUi<'a> {
    pub fn new(label: &'a str) -> Self {
        Self { label }
    }
}

impl<'a> Widget for PlaceholderUi<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        egui::Frame::group(ui.style())
            .show(ui, |ui| {
                ui.vertical_centered_justified(|ui| {
                    ui.centered_and_justified(|ui| ui.heading(self.label))
                })
            })
            .response
    }
}
