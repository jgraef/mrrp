use human_units::si::FormatSi;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::Widget,
};

use crate::util::FrequencyBand;

// todo: more precise name
#[derive(Clone, Copy, Debug)]
pub struct Frequencies {
    pub view_frequency_band: FrequencyBand,
}

impl Widget for Frequencies {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        for x in 0..area.width {
            buf[(x + area.x, area.y)].reset();
        }

        let frequency_start =
            human_units::si::Frequency::from_si(self.view_frequency_band.start.into())
                .format_si()
                .to_string();
        let frequency_center =
            human_units::si::Frequency::from_si(self.view_frequency_band.center().into())
                .format_si()
                .to_string();
        let frequency_center_pos =
            area.x + (area.width - u16::try_from(frequency_center.len()).unwrap()) / 2;
        let frequency_end =
            human_units::si::Frequency::from_si(self.view_frequency_band.end.into())
                .format_si()
                .to_string();
        let frequency_end_pos = area.x + (area.width - u16::try_from(frequency_end.len()).unwrap());

        if usize::from(area.width) > frequency_center.len() {
            buf.set_string(frequency_center_pos, 0, &frequency_center, Color::White);
        }
        if usize::from(area.width)
            > frequency_center.len() + frequency_start.len() + frequency_end.len() + 10
        {
            buf.set_string(0, 0, &frequency_start, Color::White);
            buf.set_string(frequency_end_pos, 0, &frequency_end, Color::White);
        }
    }
}
