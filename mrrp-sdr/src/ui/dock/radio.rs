use egui::{
    FontFamily,
    Frame,
    Margin,
};
use mrrp_widgets::frequency_dial::{
    FrequencyDial,
    FrequencyDialStyle,
};

#[derive(Debug)]
pub struct RadioDockView;

impl RadioDockView {
    pub fn show(self, ui: &mut egui::Ui) {
        // test
        let id = egui::Id::new("test_frequency");
        let mut frequency = ui.data(|data| data.get_temp(id).unwrap_or(7250000));

        // todo: use theme
        let response = Frame::dark_canvas(ui.style())
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.take_available_width();
                ui.add(
                    FrequencyDial::new(&mut frequency)
                        .insignificant_digits(3)
                        .desired_width(ui.available_width())
                        .style({
                            // todo: remove this. instead let the user just configure a font file -
                            // maybe through a theme config
                            let font_family = FontFamily::Name("dseg".into());

                            let mut style = FrequencyDialStyle::from_egui(ui.style());
                            style.font_id.family = font_family.clone();
                            style.small_font_id.family = font_family;
                            style.digit_spacing = 4.0;
                            style.italics = true;
                            style
                        }),
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
