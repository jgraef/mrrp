use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Color,
    widgets::{
        Block,
        Widget,
    },
};

#[derive(Debug)]
pub struct FrequencyDial<'a> {
    pub frequency: u32,
    pub title: &'a str,
}

impl<'a> Widget for FrequencyDial<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let block = Block::bordered().title(self.title);
        let inner = block.inner(area);
        block.render(area, buf);

        let text = format!("{:>width$}", self.frequency, width = inner.width.into());
        buf.set_stringn(inner.x, inner.y, &text, inner.width.into(), Color::White);
    }
}
