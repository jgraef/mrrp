use egui::{
    Frame,
    Margin,
};

use crate::ui::widgets::frequency_dial::FrequencyDial;

#[derive(Debug)]
pub struct RadioDock;

impl RadioDock {
    pub fn show(self, ui: &mut egui::Ui) {
        // test
        let id = egui::Id::new("test_frequency");
        let mut frequency = ui.data(|data| data.get_temp(id).unwrap_or(7250000));

        let response = Frame::dark_canvas(ui.style())
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.take_available_width();
                ui.add(
                    FrequencyDial::new(&mut frequency)
                        .insignificant_digits(3)
                        .desired_width(ui.available_width()),
                )
            })
            .inner;

        if response.changed() {
            tracing::debug!(?frequency, "frequency changed");
            ui.data_mut(|data| data.insert_temp(id, frequency));
        }

        ui.label("TODO: Radio");
    }
}
