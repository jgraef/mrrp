#[derive(Debug)]
pub struct ConstellationView {
    // todo
}

impl egui::Widget for ConstellationView {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        ui.scope(|ui| {
            ui.label("TODO");
            ui.take_available_space();
        })
        .response
    }
}
