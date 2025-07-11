use std::collections::VecDeque;

use human_units::si::FormatSi;
use num_complex::Complex;
use ratatui::{
    buffer::Buffer,
    layout::{
        Position,
        Rect,
    },
    palette::Hsl,
    style::Color,
    widgets::Widget,
};

#[derive(Debug)]
pub struct Waterfall {
    new_line: Option<NewLine>,
    lines: VecDeque<Line>,
    width: usize,
    height: usize,
    min_z: f32,
    max_z: f32,
    sample_rate: f32,
    center_frequency: f32,
}

impl Waterfall {
    pub fn new(min_z: f32, max_z: f32, sample_rate: f32, center_frequency: f32) -> Self {
        tracing::debug!(min_z, max_z, sample_rate, center_frequency, "waterfall");

        Self {
            new_line: None,
            lines: VecDeque::new(),
            width: 0,
            height: 0,
            min_z,
            max_z,
            sample_rate,
            center_frequency,
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn scroll(&mut self) {
        if let Some(line) = self.new_line.take() {
            while self.lines.len() >= self.height {
                self.lines.pop_front();
            }

            self.lines
                .push_back(Line::from_new_line(line, self.sample_rate));
        }
    }

    fn color(&self, z: f32) -> Color {
        let scaled = (z - self.min_z) / (self.max_z - self.min_z);
        let hue = -120.0 * (1.0 - scaled).clamp(0.0, 1.0);
        Color::from_hsl(Hsl::new(hue, 1.0, 0.5))
    }

    pub fn push(&mut self, spectrum: &[Complex<f32>]) {
        let line = self
            .new_line
            .get_or_insert_with(|| NewLine::new(self.width));
        let n = line.fft.len().min(spectrum.len());

        for i in 0..n {
            // according to rustfft the returned spectrum is not normalized.
            line.fft[i] += spectrum[i].norm_sqr();
        }
        line.count += 1;
    }

    pub fn widget(&mut self, mouse_position: Option<Position>) -> WaterfallWidget<'_> {
        WaterfallWidget {
            waterfall: self,
            mouse_position,
        }
    }
}

#[derive(Debug)]
pub struct WaterfallWidget<'a> {
    waterfall: &'a mut Waterfall,
    mouse_position: Option<Position>,
}

impl<'a> Widget for WaterfallWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.waterfall.width = area.width.into();
        self.waterfall.height = area.height.into();

        let mut total_min_max: Option<(f32, f32)> = None;
        for line_min_max in self
            .waterfall
            .lines
            .iter()
            .flat_map(|line| &line.min_max)
            .copied()
        {
            if let Some(total_min_max) = &mut total_min_max {
                total_min_max.0 = total_min_max.0.min(line_min_max.0);
                total_min_max.1 = total_min_max.1.max(line_min_max.1);
            }
            else {
                total_min_max = Some(line_min_max);
            }
        }

        if let Some((min, max)) = total_min_max {
            self.waterfall.min_z = min;
            self.waterfall.max_z = max;
        }

        for y in 0..self.waterfall.height {
            if let Some(line) = self
                .waterfall
                .lines
                .len()
                .checked_sub(y + 1)
                .map(|i| &self.waterfall.lines[i])
            {
                for (x, z) in line.fft.iter().enumerate() {
                    let position = Position {
                        x: u16::try_from(x).unwrap() + area.x,
                        y: u16::try_from(y).unwrap() + area.y,
                    };
                    if let Some(cell) = buf.cell_mut(position) {
                        cell.bg = self.waterfall.color(*z);
                    }
                }
            }
            else {
                // we basically only need to do this once when we render first, but this also
                // won't run once the screen is filled.
                for x in 0..self.waterfall.width {
                    let position = Position {
                        x: u16::try_from(x).unwrap() + area.x,
                        y: u16::try_from(y).unwrap() + area.y,
                    };
                    buf[position].bg = Color::Black;
                }
            }
        }

        if let Some(mouse_position) = self.mouse_position {
            if area.contains(mouse_position) {
                let x = usize::from(mouse_position.x - area.x);
                let y = usize::from(mouse_position.y - area.y);
                if let Some(line) = self.waterfall.lines.get(y) {
                    if x < line.fft.len() {
                        buf.set_string(
                            mouse_position.x,
                            mouse_position.y,
                            format!(
                                "x-[{} ± {}]",
                                human_units::si::Frequency::from_si(
                                    line.bin_mid_frequency(x, self.waterfall.center_frequency)
                                        as u64
                                )
                                .format_si(),
                                human_units::si::Frequency::from_si(
                                    (0.5 * line.bin_width_in_hz) as u64
                                )
                                .format_si()
                            ),
                            Color::White,
                        );
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct NewLine {
    fft: Vec<f32>,
    count: usize,
}

impl NewLine {
    fn new(width: usize) -> Self {
        Self {
            fft: vec![0.0; width],
            count: 0,
        }
    }
}

#[derive(Debug)]
struct Line {
    fft: Vec<f32>,
    min_max: Option<(f32, f32)>,
    bin_width_in_hz: f32,
}

impl Line {
    fn from_new_line(mut line: NewLine, sample_rate: f32) -> Self {
        // z is the energy for that frequency over line.count * sample_rate / len(line).
        // convert to power in dBFS.
        let bin_width_in_hz = sample_rate / line.fft.len() as f32;
        let normalize = 1.0 / (line.count as f32 * bin_width_in_hz);
        let mut min_max: Option<(f32, f32)> = None;

        for z in &mut line.fft {
            *z = 10.0 * (*z * normalize).log10();
            if let Some(min_max) = &mut min_max {
                min_max.0 = min_max.0.min(*z);
                min_max.1 = min_max.1.max(*z);
            }
            else {
                min_max = Some((*z, *z));
            }
        }

        Self {
            fft: line.fft,
            min_max,
            bin_width_in_hz,
        }
    }

    fn bin_mid_frequency(&self, index: usize, center_frequency: f32) -> f32 {
        self.bin_width_in_hz * (index as f32 + 0.5 - 0.5 * (self.fft.len() as f32))
            + center_frequency
    }
}
