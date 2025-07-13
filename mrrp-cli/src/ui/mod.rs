pub mod bandplan;
pub mod frequencies;
pub mod keybinds;
pub mod waterfall;

use crossterm::event::{
    Event as TerminalEvent,
    MouseEventKind,
};
use num_complex::Complex;
use ratatui::{
    buffer::Buffer,
    layout::{
        Constraint,
        Layout,
        Position,
        Rect,
    },
    widgets::Widget,
};

use crate::{
    app::AppProxy,
    ui::{
        bandplan::{
            Bandplan,
            BandplanWidget,
        },
        frequencies::Frequencies,
        keybinds::Keybinds,
        waterfall::Waterfall,
    },
    util::{
        FrequencyBand,
        StaticOrArc,
    },
};

#[derive(Debug)]
pub struct Ui {
    layout: Layout,

    mouse_position: Option<Position>,
    exit_requested: bool,

    keybinds: Keybinds,
    sampled_frequency_band: FrequencyBand,
    view_frequency_band: FrequencyBand,
    zoom_level: u32,
    bandwidth_resolution: f32,

    waterfall: Waterfall,
    bandplan: StaticOrArc<Bandplan>,
}

impl Ui {
    pub fn new(sampled_frequency_band: FrequencyBand) -> Self {
        Self {
            layout: Layout::vertical([
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Fill(100),
            ]),
            mouse_position: None,
            exit_requested: false,
            keybinds: Keybinds::default(),
            sampled_frequency_band,
            view_frequency_band: sampled_frequency_band,
            zoom_level: 0,
            bandwidth_resolution: 1.0,
            waterfall: Waterfall::new(sampled_frequency_band),
            bandplan: Bandplan::international().into(),
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

    pub fn handle_event(&mut self, event: UiEvent, app: &AppProxy) {
        match event {
            UiEvent::Terminal(event) => self.handle_terminal_event(event, app),
            UiEvent::ScrollWaterfall => {
                self.waterfall.scroll();
            }
            UiEvent::Spectrum {
                spectrum,
                frequency_band,
            } => {
                self.sampled_frequency_band = frequency_band;
                self.waterfall.push(spectrum, frequency_band);
            }
        }
    }

    fn handle_terminal_event(&mut self, event: crossterm::event::Event, app: &AppProxy) {
        match event {
            TerminalEvent::Key(key_event) => {
                if let Some(action) = self.keybinds.get(key_event) {
                    match action {
                        keybinds::Action::Quit => app.request_exit(),
                        keybinds::Action::ZoomIn => self.zoom_view(1),
                        keybinds::Action::ZoomOut => self.zoom_view(-1),
                        keybinds::Action::MoveLeft => self.move_view(-1, false),
                        keybinds::Action::MoveLeftBig => self.move_view(-1, true),
                        keybinds::Action::MoveRight => self.move_view(1, false),
                        keybinds::Action::MoveRightBig => self.move_view(1, true),
                        keybinds::Action::CenterView => self.center_view(),
                    }
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
                    MouseEventKind::ScrollDown => {
                        self.zoom_view(-1);
                    }
                    MouseEventKind::ScrollUp => {
                        self.zoom_view(1);
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

    fn zoom_view(&mut self, delta: i32) {
        self.zoom_level = self.zoom_level.saturating_add_signed(delta).min(30);

        // the sampled bandwidth corresponds to zoom 0
        let half = self.sampled_frequency_band.bandwidth() / 2;

        // linear zoom
        //let half_visible = half / (1 + self.zoom_level);

        // exponential zoom
        let half_visible = half / (1 << self.zoom_level);

        // keep center, but adjust visible bandwidth
        let center = self.view_frequency_band.center();
        self.view_frequency_band.start = center - half_visible;
        self.view_frequency_band.end = center + half_visible;
    }

    fn move_view(&mut self, direction: i32, big_step: bool) {
        let bandwidth = self.view_frequency_band.bandwidth();

        let step_size = if big_step {
            // move half the bandwidth
            bandwidth as i32 / 2
        }
        else {
            // move one cell worth of bandwidth
            //self.bandwidth_resolution.ceil() as i32
            bandwidth as i32 / 16
        };

        self.view_frequency_band.start = self
            .view_frequency_band
            .start
            .saturating_add_signed(direction * step_size);
        self.view_frequency_band.end = self.view_frequency_band.start + bandwidth;
    }

    fn center_view(&mut self) {
        self.zoom_level = 0;
        self.view_frequency_band = self.sampled_frequency_band;
    }
}

impl Widget for &mut Ui {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let [frequencies_area, bandplan_area, waterfall_area] = self.layout.areas(area);

        Frequencies {
            view_frequency_band: self.view_frequency_band,
        }
        .render(frequencies_area, buf);

        BandplanWidget {
            bandplan: &self.bandplan,
            view_frequency_band: self.view_frequency_band,
        }
        .render(bandplan_area, buf);

        self.waterfall
            .widget(
                self.view_frequency_band,
                self.mouse_position_inside_area(waterfall_area),
            )
            .render(waterfall_area, buf);

        self.bandwidth_resolution = self.view_frequency_band.bandwidth() as f32 / area.width as f32;
    }
}

#[derive(Debug)]
pub enum UiEvent<'a> {
    Terminal(TerminalEvent),
    ScrollWaterfall,
    Spectrum {
        spectrum: &'a [Complex<f32>],
        frequency_band: FrequencyBand,
    },
}
