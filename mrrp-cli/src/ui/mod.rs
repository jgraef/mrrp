use crossterm::event::{
    Event as TerminalEvent,
    KeyCode,
    MouseEventKind,
};
use num_complex::Complex;
use ratatui::{
    layout::{
        Constraint,
        Layout,
        Position,
        Rect,
    },
    widgets::Widget,
};

use crate::{
    ui::{
        frequencies::Frequencies,
        waterfall::Waterfall,
    },
    util::FrequencyBand,
};

pub mod frequencies;
pub mod waterfall;

#[derive(Debug)]
pub struct Ui {
    layout: Layout,

    mouse_position: Option<Position>,
    exit_requested: bool,

    view_frequency_band: FrequencyBand,

    waterfall: Waterfall,
}

impl Ui {
    pub fn new(sampled_frequency_band: FrequencyBand) -> Self {
        Self {
            layout: Layout::vertical([Constraint::Length(1), Constraint::Fill(100)]),
            mouse_position: None,
            exit_requested: false,
            view_frequency_band: sampled_frequency_band,
            waterfall: Waterfall::new(sampled_frequency_band),
        }
    }

    fn mouse_position_inside_area(&self, area: Rect) -> Option<Position> {
        self.mouse_position.and_then(|mouse_position| {
            mouse_position
                .x
                .checked_sub(area.x)
                .zip(mouse_position.y.checked_sub(area.y))
                .filter(|(x, y)| *x < area.width && *y < area.height)
                .map(|(x, y)| Position { x, y })
        })
    }

    pub fn exit_requested(&self) -> bool {
        self.exit_requested
    }

    pub fn handle_event(&mut self, event: Event) {
        match event {
            Event::Terminal(event) => self.handle_terminal_event(event),
            Event::ScrollWaterfall => {
                self.waterfall.scroll();
            }
            Event::Spectrum { spectrum } => {
                self.waterfall.push(spectrum);
            }
        }
    }

    fn handle_terminal_event(&mut self, event: crossterm::event::Event) {
        match event {
            TerminalEvent::Key(key_event) => {
                match key_event.code {
                    KeyCode::Char('q') => {
                        self.exit_requested = true;
                    }
                    _ => {}
                }
            }
            TerminalEvent::Mouse(mouse_event) => {
                match mouse_event.kind {
                    MouseEventKind::Moved => {
                        self.mouse_position = Some(Position {
                            x: mouse_event.column,
                            y: mouse_event.row,
                        });
                    }
                    _ => {}
                }
            }
            TerminalEvent::FocusLost => {
                self.mouse_position = None;
            }
            _ => {}
        }
    }
}

impl Widget for &mut Ui {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let [frequencies_area, waterfall_area] = self.layout.areas(area);

        Frequencies {
            view_frequency_band: self.view_frequency_band,
        }
        .render(frequencies_area, buf);

        self.waterfall
            .widget(self.mouse_position_inside_area(waterfall_area))
            .render(waterfall_area, buf);
    }
}

#[derive(Debug)]
pub enum Event<'a> {
    Terminal(TerminalEvent),
    ScrollWaterfall,
    Spectrum { spectrum: &'a [Complex<f32>] },
}
