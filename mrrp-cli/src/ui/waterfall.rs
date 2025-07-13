use std::collections::VecDeque;

use num_complex::Complex;
use palette::Srgb;
use ratatui::{
    buffer::{
        Buffer,
        Cell,
    },
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
    format_frequency,
    lerp,
    max_float,
    min_float,
    unlerp,
};

#[derive(Debug)]
pub struct Waterfall {
    new_line: Option<NewLine>,
    lines: Lines,
    sampled_frequency_band: FrequencyBand,
    cache: Option<Cache>,

    // todo: move this into the widget?
    color_map: ColorMap,
    downsampling: Downsampling,
}

impl Waterfall {
    pub fn new(sampled_frequency_band: FrequencyBand) -> Self {
        Self {
            new_line: None,
            lines: Lines::new(10),
            color_map: ColorMap::default(),
            sampled_frequency_band,
            cache: Some(Cache::default()),
            //cache: None,
            downsampling: Downsampling::Average,
        }
    }

    pub fn scroll(&mut self) {
        if let Some(line) = self.new_line.take() {
            if let Some(line) = line.into_line() {
                self.lines.push(line);

                if let Some(cache) = &mut self.cache {
                    cache.scroll(self.lines.history);
                }
            }
        }
    }

    pub fn push(&mut self, spectrum: &[Complex<f32>], sampled_frequency_band: FrequencyBand) {
        if sampled_frequency_band != self.sampled_frequency_band {
            self.scroll();
            self.sampled_frequency_band = sampled_frequency_band;
            panic!("not yet!");
        }

        let line = self
            .new_line
            .get_or_insert_with(|| NewLine::new(spectrum.len(), self.sampled_frequency_band));

        assert_eq!(line.samples.len(), spectrum.len(), "fft size changed");
        assert_eq!(
            sampled_frequency_band, self.sampled_frequency_band,
            "sampled frequency band mismatch"
        );

        for i in 0..line.samples.len() {
            line.samples[i] += spectrum[i].norm_sqr();
        }
        line.count += 1;
    }

    pub fn widget<'a>(
        &'a mut self,
        view_frequency_band: FrequencyBand,
        mouse_position: Option<Position>,
    ) -> WaterfallWidget<'a> {
        WaterfallWidget {
            waterfall: self,
            view_frequency_band,
            mouse_position,
        }
    }
}

#[derive(Debug)]
pub struct WaterfallWidget<'a> {
    waterfall: &'a mut Waterfall,
    view_frequency_band: FrequencyBand,
    mouse_position: Option<Position>,
}

impl<'a> Widget for WaterfallWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.waterfall.lines.history = area.height.max(10).into();

        let mut mouse_over = None;
        let mut total_min_max = None;
        let display_bin_width = self.view_frequency_band.bandwidth() as f32 / area.width as f32;

        let sample_spectrum = |x: u16, line: &Line| {
            let start_frequency =
                self.view_frequency_band.start as f32 + x as f32 * display_bin_width;
            let end_frequency =
                self.view_frequency_band.start as f32 + (x + 1) as f32 * display_bin_width;

            let start_line_index = (((start_frequency - line.frequency_band.start as f32)
                / line.bin_width)
                .max(0.0) as usize)
                .min(line.samples.len());

            let end_line_index = (((end_frequency - line.frequency_band.start as f32)
                / line.bin_width)
                .ceil()
                .max(0.0) as usize)
                .min(line.samples.len());

            (start_line_index < end_line_index).then(|| {
                let samples = &line.samples[start_line_index..end_line_index];
                self.waterfall.downsampling.apply(samples)
            })
        };

        let mut render_cell = |x, y, z, buf: &mut Buffer| {
            if let Some(z) = z {
                // render to cell
                buf[(area.x + x, area.y + y)].bg = self.waterfall.color_map.map(z);

                // track min max
                if let Some((min, max)) = &mut total_min_max {
                    assert!(
                        z.is_finite(),
                        "so z can be infinite. interesting. i mean of course it can. it comes out of a log"
                    );
                    if z < *min {
                        *min = z;
                    }
                    if z > *max {
                        *max = z;
                    }
                }
                else {
                    total_min_max = Some((z, z));
                }

                // get mouse over info if the mouse is over this cell
                if self.mouse_position.map_or(false, |mouse_position| {
                    mouse_position.x == x && mouse_position.y == y
                }) {
                    let start_frequency = (self.view_frequency_band.start as f32
                        + x as f32 * display_bin_width)
                        as u32;
                    let end_frequency = (self.view_frequency_band.start as f32
                        + (x + 1) as f32 * display_bin_width)
                        as u32;

                    mouse_over = Some((
                        z,
                        FrequencyBand {
                            start: start_frequency,
                            end: end_frequency,
                        },
                    ));
                }
            }
            else {
                clear_cell(&mut buf[(x + area.x, y + area.y)]);
            }
        };

        // render spectral density history
        for y in 0..area.height {
            if let Some(line) = self.waterfall.lines.get_line(y.into()) {
                if let Some(cache) = &mut self.waterfall.cache {
                    let cache_line = cache.get_line(y, area.width, self.view_frequency_band, |x| {
                        sample_spectrum(x, line)
                    });

                    for x in 0..area.width {
                        render_cell(x, y, cache_line[usize::from(x)], buf);
                    }
                }
                else {
                    for x in 0..area.width {
                        render_cell(x, y, sample_spectrum(x, line), buf);
                    }
                }
            }
            else {
                for x in 0..area.width {
                    clear_cell(&mut buf[(x + area.x, y + area.y)]);
                }
            }
        }

        // update colormap min/max values for next frame
        // todo: this should be behind some flag
        if let Some((min, max)) = total_min_max {
            self.waterfall.color_map.min_z = min;
            self.waterfall.color_map.max_z = max;
        }

        // render mouse cursor
        if let Some((mouse_position, (z, frequency_band))) = self.mouse_position.zip(mouse_over) {
            let text = format!(
                "x-[{} ± {}: {:.1} dBFS]-x",
                format_frequency(frequency_band.center()).with_band(self.view_frequency_band),
                format_frequency(frequency_band.bandwidth() / 2),
                z,
            );
            let text_width = text.len() - 4;

            if usize::from(mouse_position.x) + text_width > area.width.into()
                && usize::from(mouse_position.x) > text_width
            {
                buf.set_string(
                    area.x + mouse_position.x - u16::try_from(text_width).unwrap(),
                    area.y + mouse_position.y,
                    &text[2..],
                    Color::White,
                );
            }
            else {
                buf.set_string(
                    area.x + mouse_position.x,
                    area.y + mouse_position.y,
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
            // todo: this needs some serious verification lol. (yeah it is wrong, also check
            // the initial fft normalization)

            // according to [this][1] we can divide by bin with to get the "dB power
            // spectral density" instead of "dB power"
            //
            // and we need to divide by the length of the sampled signal ([2])
            //
            // [1]: https://dsp.stackexchange.com/questions/19615/converting-raw-i-q-to-db
            // [2]: https://stackoverflow.com/questions/20165193/fft-normalization

            // dB power spectral density
            // dividing by bin width and num samples, cancles out the num samples from both
            // terms

            // let normalize = 1.0 / (self.count as f32 * self.bin_width *
            // self.samples.len() as f32);
            let normalize = 1.0 / (self.count as f32 * self.frequency_band.bandwidth() as f32);

            // dB power
            //let normalize = 1.0 / (self.count as f32 * self.samples.len() as f32);

            for z in &mut self.samples {
                *z = 10.0 * (*z * normalize).log10();
            }

            Some(Line {
                samples: self.samples,
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

        // -120 blue
        // 0 red
        // 120 green

        Self::new(-120.0, 0.0)
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
        let normalized = unlerp(z, self.min_z, self.max_z).clamp(0.0, 1.0);

        let hue = lerp(normalized, self.hue_low, self.hue_high);
        let saturation = 1.0;
        let lightness = lerp(normalized.powi(2), 0.0, 0.5);

        Color::from_hsl(Hsl::new(hue, saturation, lightness))
    }
}

// todo: this must be carefully choosen if we want conserved quantities.
// basically if we're doing density this should be average, min or max.
// otherwise sum.
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

fn clear_cell(cell: &mut Cell) {
    cell.reset();
    cell.bg = Srgb::<f32>::default().into();
}

#[derive(Debug)]
struct Lines {
    lines: VecDeque<Line>,
    history: usize,
}

impl Lines {
    pub fn new(history: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(history),
            history,
        }
    }

    pub fn push(&mut self, line: Line) {
        while self.lines.len() >= self.history && !self.lines.is_empty() {
            // todo: reuse those poor buffers :cryring:
            self.lines.pop_front();
        }

        self.lines.push_back(line);
    }

    pub fn get_line(&self, i: usize) -> Option<&Line> {
        self.lines.len().checked_sub(i + 1).map(|i| &self.lines[i])
    }
}

#[derive(Debug, Default)]
struct Cache {
    lines: VecDeque<Vec<Option<f32>>>,
    view_frequency_band: Option<FrequencyBand>,
}

impl Cache {
    pub fn scroll(&mut self, history: usize) {
        self.lines.push_front(vec![]);
        while self.lines.len() > history {
            self.lines.pop_back();
        }
    }

    pub fn get_line(
        &mut self,
        y: u16,
        width: u16,
        view_frequency_band: FrequencyBand,
        mut sample_spectrum: impl FnMut(u16) -> Option<f32>,
    ) -> &Vec<Option<f32>> {
        let line_index = usize::from(y);
        let line_size = usize::from(width);

        if self
            .view_frequency_band
            .map_or(true, |band| band != view_frequency_band)
        {
            self.lines.clear();
            self.view_frequency_band = Some(view_frequency_band);
        }

        // this just makes sure that if we happen to render an older line that somehow
        // (impossible!) doesn't exist yet, we make space for it.
        //
        // haha, I had the comparision the wrong way it it quickly filled all memory :D
        while line_index >= self.lines.len() {
            self.lines.push_back(vec![]);
        }

        let line = &mut self.lines[line_index];
        if line.is_empty() {
            line.reserve(line_size);
            for x in 0..width {
                line.push(sample_spectrum(x));
            }
        }

        &*line
    }
}
