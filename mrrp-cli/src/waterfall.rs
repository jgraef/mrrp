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

use crate::util::{
    FrequencyBand,
    lerp,
    max_float,
    min_float,
};

#[derive(Debug)]
pub struct Waterfall {
    new_line: Option<NewLine>,
    lines: VecDeque<Line>,
    history: usize,
    input_frequency_band: FrequencyBand,

    // todo: move this into the widget?
    view_frequency_band: FrequencyBand,
    color_map: ColorMap,
    downsampling: Downsampling,
}

impl Waterfall {
    pub fn new(input_frequency_band: FrequencyBand) -> Self {
        Self {
            new_line: None,
            lines: VecDeque::new(),
            history: 10,
            color_map: ColorMap::default(),
            input_frequency_band,
            view_frequency_band: input_frequency_band,
            downsampling: Downsampling::Max,
        }
    }

    pub fn scroll(&mut self) {
        if let Some(line) = self.new_line.take() {
            while self.lines.len() >= self.history && !self.lines.is_empty() {
                // todo: reuse those poor buffers :cryring:
                self.lines.pop_front();
            }

            if let Some(line) = line.into_line() {
                self.lines.push_back(line);
            }
        }
    }

    pub fn push(&mut self, spectrum: &[Complex<f32>]) {
        let line = self
            .new_line
            .get_or_insert_with(|| NewLine::new(spectrum.len(), self.input_frequency_band));

        assert_eq!(line.samples.len(), spectrum.len(), "fft size changed");
        for i in 0..line.samples.len() {
            line.samples[i] += spectrum[i].norm_sqr();
        }

        line.count += 1;
    }

    pub fn widget(&mut self, mouse_position: Option<Position>) -> WaterfallWidget<'_> {
        WaterfallWidget {
            waterfall: self,
            mouse_position,
        }
    }

    fn min_max(&self) -> Option<(f32, f32)> {
        let mut total_min_max: Option<(f32, f32)> = None;
        for line_min_max in self.lines.iter().flat_map(|line| &line.min_max).copied() {
            if let Some(total_min_max) = &mut total_min_max {
                total_min_max.0 = total_min_max.0.min(line_min_max.0);
                total_min_max.1 = total_min_max.1.max(line_min_max.1);
            }
            else {
                total_min_max = Some(line_min_max);
            }
        }
        total_min_max
    }

    fn get_line(&self, i: usize) -> Option<&Line> {
        self.lines.len().checked_sub(i + 1).map(|i| &self.lines[i])
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
        self.waterfall.history = area.height.saturating_sub(1).max(10).into();

        let total_min_max = self.waterfall.min_max();
        if let Some((min, max)) = total_min_max {
            self.waterfall.color_map.min_z = min;
            self.waterfall.color_map.max_z = max;
        }

        // render first line showing base frequency, center frequency and end frequency
        for x in 0..area.width {
            buf[(x + area.x, area.y)].reset();
        }

        let frequency_start =
            human_units::si::Frequency::from_si(self.waterfall.view_frequency_band.start.into())
                .format_si()
                .to_string();
        let frequency_center = human_units::si::Frequency::from_si(
            ((self.waterfall.view_frequency_band.start + self.waterfall.view_frequency_band.end)
                / 2)
            .into(),
        )
        .format_si()
        .to_string();
        let frequency_center_pos =
            area.x + (area.width - u16::try_from(frequency_center.len()).unwrap()) / 2;
        let frequency_end =
            human_units::si::Frequency::from_si(self.waterfall.view_frequency_band.end.into())
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

        let mut mouse_over = None;

        // render spectral density history
        for y in 1..area.height {
            if let Some(line) = self.waterfall.get_line((y - 1).into()) {
                // offset and len of samples we take from line.samples
                let samples_start = 0;
                let samples_len = line.samples.len();

                // how many samples we render
                let display_len = usize::from(area.width);

                if display_len < samples_len {
                    for i in 0..display_len {
                        // sum into one value
                        // todo: what is the expected behavior here? sum or average?
                        // okay, we average here now. otherwise the auto-scaling for the color-map
                        // doesn't match.
                        let j1 = i * samples_len / display_len + samples_start;
                        let j2 = (i + 1) * samples_len / display_len + samples_start;
                        assert!(j2 > j1);

                        let samples = &line.samples[j1..j2];
                        let z = self.waterfall.downsampling.apply(samples);

                        // render to cell
                        let x = u16::try_from(i).unwrap();
                        buf[(area.x + x, area.y + y)].bg = self.waterfall.color_map.map(z);

                        if self.mouse_position.map_or(false, |mouse_position| {
                            mouse_position.x == x && mouse_position.y == y
                        }) {
                            mouse_over = Some((
                                z,
                                FrequencyBand {
                                    start: (j1 as f32 * line.bin_width) as u32
                                        + line.frequency_band.start,
                                    end: (j2 as f32 * line.bin_width) as u32
                                        + line.frequency_band.start,
                                },
                            ));
                        }
                    }
                }
                else {
                    // todo: basically do the opposite of the code above - filling multiple cells
                    // from one sample
                    tracing::debug!(line_samples = line.samples.len(), samples_len, "todo");
                    todo!();
                }
            }
            else {
                // we basically only need to do this once when we render first, but this also
                // won't run once the screen is filled.
                for x in 0..area.width {
                    buf[(x + area.x, y + area.y)].reset();
                }
            }
        }

        // render mouse cursor
        if let Some((mouse_position, (z, frequency_band))) = self.mouse_position.zip(mouse_over) {
            let text = format!(
                "x-[{} ± {}: {:.1} dBFS]-x",
                human_units::si::Frequency::from_si(frequency_band.center().into()).format_si(),
                human_units::si::Frequency::from_si((frequency_band.bandwidth() / 2).into())
                    .format_si(),
                z,
            );
            let text_width = text.len() - 4;

            if usize::from(mouse_position.x) + text_width > area.width.into()
                && usize::from(mouse_position.x) > text_width
            {
                buf.set_string(
                    mouse_position.x - u16::try_from(text_width).unwrap(),
                    mouse_position.y,
                    &text[2..],
                    Color::White,
                );
            }
            else {
                buf.set_string(
                    mouse_position.x,
                    mouse_position.y,
                    &text[..text_width + 2],
                    Color::White,
                );
            }
        }
    }
}

#[derive(Debug)]
struct NewLine {
    samples: Vec<f32>,
    count: usize,
    frequency_band: FrequencyBand,
    bin_width: f32,
}

impl NewLine {
    fn new(width: usize, frequency_band: FrequencyBand) -> Self {
        let bin_width = frequency_band.bandwidth() as f32 / width as f32;
        Self {
            samples: vec![0.0; width],
            count: 0,
            frequency_band,
            bin_width,
        }
    }

    fn into_line(mut self) -> Option<Line> {
        if self.count > 0 {
            // z is the energy for that frequency over line.count * sample_rate / len(line).
            // convert to power in dBFS.
            let normalize = 1.0 / (self.count as f32 * self.bin_width);
            //let normalize = 1.0 / (self.count as f32);
            let mut min_max: Option<(f32, f32)> = None;

            for z in &mut self.samples {
                *z = 10.0 * (*z * normalize).log10();
                if z.is_finite() {
                    if let Some(min_max) = &mut min_max {
                        min_max.0 = min_max.0.min(*z);
                        min_max.1 = min_max.1.max(*z);
                    }
                    else {
                        min_max = Some((*z, *z));
                    }
                }
            }

            Some(Line {
                samples: self.samples,
                min_max,
                frequency_band: self.frequency_band,
                bin_width: self.bin_width,
            })
        }
        else {
            None
        }
    }
}

#[derive(Debug)]
struct Line {
    samples: Vec<f32>,
    min_max: Option<(f32, f32)>,
    frequency_band: FrequencyBand,
    bin_width: f32,
}

#[derive(Debug)]
struct ColorMap {
    min_z: f32,
    max_z: f32,
    hue_low: f32,
    hue_high: f32,
}

impl Default for ColorMap {
    fn default() -> Self {
        // blue -> green -> red
        //Self::new(240.0, 0.0)
        // blue -> red -> green
        Self::new(-120.0, 120.0)
    }
}

impl ColorMap {
    pub fn new(hue_low: f32, hue_high: f32) -> Self {
        // in dBFS, pulled these out of my ass. they will get updated anyway. just don't
        // divide by 0, mkay.
        let min_z = -80.0;
        let max_z = -70.0;

        Self {
            min_z,
            max_z,
            hue_low,
            hue_high,
        }
    }

    pub fn map(&self, z: f32) -> Color {
        let scaled = ((z - self.min_z) / (self.max_z - self.min_z)).clamp(0.0, 1.0);
        let hue = lerp(scaled, self.hue_low, self.hue_high);
        Color::from_hsl(Hsl::new(hue, 1.0, 0.8 * scaled))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Downsampling {
    Sum,
    Average,
    Min,
    #[default]
    Max,
}

impl Downsampling {
    pub fn apply(&self, samples: &[f32]) -> f32 {
        assert!(samples.len() > 0);
        match self {
            Downsampling::Sum => samples.iter().sum(),
            Downsampling::Average => samples.iter().sum::<f32>() / samples.len() as f32,
            Downsampling::Min => min_float(samples.iter().copied()).unwrap(),
            Downsampling::Max => max_float(samples.iter().copied()).unwrap(),
        }
    }
}
