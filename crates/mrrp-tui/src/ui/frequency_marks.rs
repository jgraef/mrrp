use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::Widget,
};

use crate::util::{
    FrequencyBand,
    format_frequency,
};

// todo: more precise name
#[derive(Clone, Copy, Debug)]
pub struct FrequencyMarks {
    pub view_frequency_band: FrequencyBand,
}

impl Widget for FrequencyMarks {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        for x in 0..area.width {
            buf[(x + area.x, area.y)].reset();
        }

        let frequency_start = format_frequency(self.view_frequency_band.start)
            .with_band(self.view_frequency_band)
            .to_string();
        let frequency_center = format_frequency(self.view_frequency_band.center())
            .with_band(self.view_frequency_band)
            .to_string();
        let frequency_center_pos =
            area.x + (area.width - u16::try_from(frequency_center.len()).unwrap()) / 2;
        let frequency_end = format_frequency(self.view_frequency_band.end)
            .with_band(self.view_frequency_band)
            .to_string();
        let frequency_end_pos = area.x + (area.width - u16::try_from(frequency_end.len()).unwrap());

        if usize::from(area.width) > frequency_center.len() {
            buf.set_string(
                area.x + frequency_center_pos,
                area.y,
                &frequency_center,
                Color::White,
            );
        }
        if usize::from(area.width)
            > frequency_center.len() + frequency_start.len() + frequency_end.len() + 10
        {
            buf.set_string(area.x, area.y, &frequency_start, Color::White);
            buf.set_string(
                area.x + frequency_end_pos,
                area.y,
                &frequency_end,
                Color::White,
            );
        }
    }
}
