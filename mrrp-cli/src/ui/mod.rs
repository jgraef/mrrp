pub mod bandplan;
pub mod frequency_dial;
pub mod frequency_marks;
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
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    app::AppProxy,
    ui::{
        bandplan::{
            Bandplan,
            BandplanWidget,
        },
        frequency_dial::FrequencyDial,
        frequency_marks::FrequencyMarks,
        keybinds::Keybinds,
        waterfall::{
            WaterfallState,
            WaterfallWidget,
        },
    },
    util::FrequencyBand,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct UiState {
    view_frequency_band: FrequencyBand,
    zoom_level: u32,
    waterfall_state: WaterfallState,
}

impl UiState {
    pub fn new(view_frequency_band: FrequencyBand) -> Self {
        Self {
            view_frequency_band,
            zoom_level: 0,
            waterfall_state: WaterfallState::default(),
        }
    }
}

#[derive(Debug)]
pub struct Ui {
    layout: Layout,

    mouse_position: Option<Position>,
    exit_requested: bool,

    keybinds: Keybinds,
    bandplan: Bandplan,
    sampled_frequency_band: FrequencyBand,
    bandwidth_resolution: f32,

    state: UiState,
}

impl Ui {
    pub fn new(
        sampled_frequency_band: FrequencyBand,
        keybinds: Keybinds,
        bandplan: Bandplan,
        ui_state: Option<UiState>,
    ) -> Self {
        Self {
            layout: Layout::vertical([
                Constraint::Length(3),
                Constraint::Length(1),
                Constraint::Length(1),
                Constraint::Fill(100),
            ]),
            mouse_position: None,
            exit_requested: false,
            keybinds,
            sampled_frequency_band,
            bandwidth_resolution: 1.0,
            bandplan,
            state: ui_state.unwrap_or_else(|| UiState::new(sampled_frequency_band)),
        }
    }

    pub fn state(&self) -> &UiState {
        &self.state
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
                self.state.waterfall_state.scroll();
            }
            UiEvent::Spectrum {
                spectrum,
                frequency_band,
            } => {
                self.sampled_frequency_band = frequency_band;
                self.state.waterfall_state.push(spectrum, frequency_band);
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
        self.state.zoom_level = self.state.zoom_level.saturating_add_signed(delta).min(30);

        // the sampled bandwidth corresponds to zoom 0
        let half = self.sampled_frequency_band.bandwidth() / 2;

        // linear zoom
        //let half_visible = half / (1 + self.zoom_level);

        // exponential zoom
        let half_visible = half / (1 << self.state.zoom_level);

        // keep center, but adjust visible bandwidth
        let center = self.state.view_frequency_band.center();
        self.state.view_frequency_band.start = center - half_visible;
        self.state.view_frequency_band.end = center + half_visible;
    }

    fn move_view(&mut self, direction: i32, big_step: bool) {
        let bandwidth = self.state.view_frequency_band.bandwidth();

        let step_size = if big_step {
            // move half the bandwidth
            bandwidth as i32 / 2
        }
        else {
            // move one cell worth of bandwidth
            //self.bandwidth_resolution.ceil() as i32
            bandwidth as i32 / 16
        };

        self.state.view_frequency_band.start = self
            .state
            .view_frequency_band
            .start
            .saturating_add_signed(direction * step_size);
        self.state.view_frequency_band.end = self.state.view_frequency_band.start + bandwidth;
    }

    fn center_view(&mut self) {
        self.state.zoom_level = 0;
        self.state.view_frequency_band = self.sampled_frequency_band;
    }
}

impl Widget for &mut Ui {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let [
            controls_area,
            frequencies_area,
            bandplan_area,
            waterfall_area,
        ] = self.layout.areas(area);

        FrequencyDial {
            frequency: self.sampled_frequency_band.center(),
            title: "Tuner",
        }
        .render(controls_area, buf);

        FrequencyMarks {
            view_frequency_band: self.state.view_frequency_band,
        }
        .render(frequencies_area, buf);

        BandplanWidget {
            bandplan: &self.bandplan,
            view_frequency_band: self.state.view_frequency_band,
        }
        .render(bandplan_area, buf);

        let waterfall_mouse_position = self.mouse_position_inside_area(waterfall_area);
        WaterfallWidget {
            waterfall: &mut self.state.waterfall_state,
            view_frequency_band: self.state.view_frequency_band,
            mouse_position: waterfall_mouse_position,
        }
        .render(waterfall_area, buf);

        self.bandwidth_resolution =
            self.state.view_frequency_band.bandwidth() as f32 / area.width as f32;
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
