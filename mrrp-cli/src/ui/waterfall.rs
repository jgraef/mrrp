use std::{
    collections::VecDeque,
    fs::File,
    io::{
        BufReader,
        BufWriter,
    },
    ops::Index,
    path::Path,
};

use num_complex::Complex;
use palette::LinSrgb;
use ratatui::{
    buffer::Buffer,
    layout::{
        Position,
        Rect,
        Size,
    },
    palette::Hsl,
    style::Color,
    widgets::Widget,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    Error,
    util::{
        FrequencyBand,
        debug_limited,
        format_frequency,
        lerp,
        max_float,
        min_float,
        unlerp,
    },
};

#[derive(Debug, Serialize, Deserialize)]
pub struct WaterfallState {
    new_line: Option<NewLine>,
    lines: Lines,
    #[serde(skip, default)]
    cache: Cache,
    downsampling: Downsampling,
    draw_mode: DrawMode,
    min_z: f32,
    max_z: f32,
}

impl Default for WaterfallState {
    fn default() -> Self {
        // in dBFS, pulled these out of my ass. they will get updated anyway. just don't
        // divide by 0, mkay.
        let min_z = -80.0;
        let max_z = -70.0;

        Self {
            new_line: None,
            lines: Lines::new(10),
            cache: Default::default(),
            downsampling: Downsampling::Max,
            draw_mode: DrawMode::HalfBlockHorizontal,
            min_z,
            max_z,
        }
    }
}

impl WaterfallState {
    pub fn scroll(&mut self) {
        if let Some(line) = self.new_line.take() {
            if let Some(line) = line.into_line() {
                self.lines.push(line);

                self.cache.scroll(self.lines.history);
            }
        }
    }

    pub fn push(&mut self, spectrum: &[Complex<f32>], sampled_frequency_band: FrequencyBand) {
        if let Some(new_line) = &mut self.new_line {
            if new_line.frequency_band != sampled_frequency_band {
                self.scroll();
            }
        }

        let new_line = self
            .new_line
            .get_or_insert_with(|| NewLine::new(spectrum.len(), sampled_frequency_band));

        assert_eq!(new_line.samples.len(), spectrum.len(), "fft size changed");
        assert_eq!(
            sampled_frequency_band, new_line.frequency_band,
            "sampled frequency band mismatch"
        );

        for i in 0..new_line.samples.len() {
            new_line.samples[i] += spectrum[i].norm_sqr();
        }
        new_line.count += 1;
    }
}

const HALF_BLOCK_LEFT: char = '\u{258c}';
const HALF_BLOCK_TOP: char = '\u{2580}';
const COLOR_BLACK: Color = Color::Rgb(0, 0, 0);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum DrawMode {
    #[default]
    FullBlock,
    HalfBlockHorizontal,
    HalfBlockVertical,
}

#[derive(Debug)]
pub struct Canvas<'a> {
    pub area: Rect,
    pub buf: &'a mut Buffer,
    pub mode: DrawMode,
    pub size: Size,
}

impl<'a> Canvas<'a> {
    pub fn new(area: Rect, buf: &'a mut Buffer, mode: DrawMode) -> Self {
        let size = match mode {
            DrawMode::FullBlock => {
                Size {
                    width: area.width,
                    height: area.height,
                }
            }
            DrawMode::HalfBlockHorizontal => {
                Size {
                    width: area.width * 2,
                    height: area.height,
                }
            }
            DrawMode::HalfBlockVertical => {
                Size {
                    width: area.width,
                    height: area.height * 2,
                }
            }
        };

        Self {
            area,
            buf,
            mode,
            size,
        }
    }

    #[inline(always)]
    pub fn draw(&mut self, position: impl Into<Position>, color: impl Into<Color>) {
        self.draw_impl(position.into(), color.into());
    }

    fn draw_impl(&mut self, position: Position, color: Color) {
        match self.mode {
            DrawMode::FullBlock => {
                self.buf[(self.area.x + position.x, self.area.y + position.y)].bg = color;
            }
            DrawMode::HalfBlockHorizontal => {
                let cell =
                    &mut self.buf[(self.area.x + (position.x / 2), self.area.y + position.y)];
                if position.x % 2 == 0 {
                    cell.fg = color;
                    cell.set_char(HALF_BLOCK_LEFT);
                }
                else {
                    cell.bg = color;
                }
            }
            DrawMode::HalfBlockVertical => {
                let cell =
                    &mut self.buf[(self.area.x + position.x, self.area.y + (position.y / 2))];
                if position.y % 2 == 0 {
                    cell.fg = color;
                    cell.set_char(HALF_BLOCK_TOP);
                }
                else {
                    cell.bg = color;
                }
            }
        }
    }

    pub fn clear(&mut self, position: impl Into<Position>) {
        self.draw_impl(position.into(), COLOR_BLACK);
    }

    pub fn clear_line(&mut self, y: u16) {
        let y = match self.mode {
            DrawMode::FullBlock => y,
            DrawMode::HalfBlockHorizontal => y,
            DrawMode::HalfBlockVertical => y / 2,
        };

        for x in 0..self.area.width {
            let cell = &mut self.buf[(self.area.x + x, self.area.y + y)];
            cell.reset();
            cell.bg = COLOR_BLACK;
        }
    }
}

#[derive(Debug)]
pub struct WaterfallWidget<'a> {
    pub waterfall: &'a mut WaterfallState,
    pub view_frequency_band: FrequencyBand,
    pub mouse_position: Option<Position>,
    pub color_map: &'a ColorMap,
}

impl<'a> Widget for WaterfallWidget<'a> {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        let mut canvas = Canvas::new(area, buf, self.waterfall.draw_mode);
        self.waterfall.lines.history = canvas.size.height.max(10).into();

        let mut total_min_max = None;
        let display_bin_width =
            self.view_frequency_band.bandwidth() as f32 / canvas.size.width as f32;

        let sample_spectrum = |x: u16, line: &Line| {
            let line_start = line.frequency_band.start as f32;

            let start_frequency =
                self.view_frequency_band.start as f32 + x as f32 * display_bin_width;
            let end_frequency =
                self.view_frequency_band.start as f32 + (x + 1) as f32 * display_bin_width;

            let start_line_index = (((start_frequency - line_start) / line.bin_width).max(0.0)
                as usize)
                .min(line.samples.len());

            let end_line_index = (((end_frequency - line_start) / line.bin_width)
                .ceil()
                .max(0.0) as usize)
                .min(line.samples.len());

            (start_line_index < end_line_index).then(|| {
                let samples = &line.samples[start_line_index..end_line_index];
                (
                    self.waterfall.downsampling.apply(samples),
                    FrequencyBand {
                        start: start_frequency as u32,
                        end: end_frequency as u32,
                    },
                )
            })
        };

        let mut render_cell = |x, y, z, canvas: &mut Canvas| {
            if let Some(z) = z {
                // render to cell
                let normalized =
                    unlerp(z, self.waterfall.min_z, self.waterfall.max_z).clamp(0.0, 1.0);
                canvas.draw((x, y), self.color_map.map(normalized));

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
            }
            else {
                canvas.clear((x, y));
            }
        };

        // render spectral density history
        for y in 0..canvas.size.height {
            if let Some(line) = self.waterfall.lines.get_line(y.into()) {
                let cache_line = self.waterfall.cache.get_line_or_sample(
                    y,
                    canvas.size.width,
                    self.view_frequency_band,
                    |x| sample_spectrum(x, line).map(|(z, _)| z),
                );

                for x in 0..canvas.size.width {
                    render_cell(x, y, cache_line[usize::from(x)], &mut canvas);
                }
            }
            else {
                canvas.clear_line(y);
            }
        }

        // update colormap min/max values for next frame
        // todo: this should be behind some flag
        if let Some((min, max)) = total_min_max {
            self.waterfall.min_z = min;
            self.waterfall.max_z = max;
        }

        // render mouse cursor
        if let Some(mouse_position) = self.mouse_position {
            if let Some(line) = self.waterfall.lines.get_line(mouse_position.y.into()) {
                // fixme: this is still broken with half-width blocks
                if let Some((z, mouse_frequency_band)) = sample_spectrum(mouse_position.x, line) {
                    let text = format!(
                        "x-[{} ± {}: {:.1} dBFS]-x",
                        format_frequency(mouse_frequency_band.center())
                            .with_band(self.view_frequency_band),
                        format_frequency(mouse_frequency_band.bandwidth() / 2),
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
    }
}

#[derive(derive_more::Debug, Serialize, Deserialize)]
struct NewLine {
    #[debug("{:?}", debug_limited(samples))]
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

#[derive(derive_more::Debug, Serialize, Deserialize)]
struct Line {
    #[debug("{:?}", debug_limited(samples))]
    samples: Vec<f32>,
    frequency_band: FrequencyBand,
    bin_width: f32,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorMap {
    HueLightness {
        hue_low: f32,
        hue_high: f32,
        lightness_low: f32,
        lightness_high: f32,
    },
    LinearRgb {
        colors: Vec<LinSrgb>,
    },
    Closest {
        colors: Vec<Color>,
    },
}

impl Default for ColorMap {
    fn default() -> Self {
        Self::HueLightness {
            hue_low: -120.0,
            hue_high: 0.0,
            lightness_low: 0.1,
            lightness_high: 0.8,
        }
    }
}

impl ColorMap {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        tracing::debug!(path = %path.as_ref().display(), "Loading colormap from file");
        Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
    }

    pub fn to_path(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        tracing::debug!(path = %path.as_ref().display(), "Wrriting colormap to file");
        serde_json::to_writer_pretty(BufWriter::new(File::create(path)?), self)?;
        Ok(())
    }

    pub fn map(&self, normalized: f32) -> Color {
        match self {
            Self::HueLightness {
                hue_low,
                hue_high,
                lightness_low,
                lightness_high,
            } => {
                Color::from_hsl(Hsl::new(
                    lerp(normalized, *hue_low, *hue_high),
                    1.0,
                    lerp(normalized.powi(2), *lightness_low, *lightness_high),
                ))
            }
            Self::LinearRgb { colors } => {
                let i = normalized * (colors.len() - 1) as f32;
                let i_low = i.floor();
                let i_high = i.ceil();
                let color_low = colors[i_low as usize];
                if i_low == i_high {
                    color_low.into()
                }
                else {
                    let t = i - i_low;
                    let color_high = colors[i_high as usize];
                    LinSrgb::new(
                        lerp(t, color_low.red, color_high.red),
                        lerp(t, color_low.green, color_high.green),
                        lerp(t, color_low.blue, color_high.blue),
                    )
                    .into()
                }
            }
            Self::Closest { colors } => {
                let i = (normalized * (colors.len() - 1) as f32).round() as usize;
                colors[i]
            }
        }
    }
}

// todo: this must be carefully choosen if we want conserved quantities.
// basically if we're doing density this should be average, min or max.
// otherwise sum.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Downsampling {
    Sum,
    #[default]
    Average,
    Min,
    Max,
    First,
}

impl Downsampling {
    pub fn apply(&self, samples: &[f32]) -> f32 {
        assert!(samples.len() > 0);
        match self {
            Downsampling::Sum => samples.iter().sum(),
            Downsampling::Average => samples.iter().sum::<f32>() / samples.len() as f32,
            Downsampling::Min => min_float(samples.iter().copied()).unwrap(),
            Downsampling::Max => max_float(samples.iter().copied()).unwrap(),
            Downsampling::First => samples[0],
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
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
    lines: VecDeque<CacheLine>,
    view_frequency_band: Option<FrequencyBand>,
    canvas_width: Option<u16>,
}

impl Cache {
    pub fn scroll(&mut self, history: usize) {
        self.lines.push_front(Default::default());
        while self.lines.len() > history {
            self.lines.pop_back();
        }
    }

    #[allow(unused)]
    pub fn clear(&mut self) {
        self.lines.clear();
        self.view_frequency_band = None;
    }

    pub fn get_line_or_sample(
        &mut self,
        y: u16,
        width: u16,
        view_frequency_band: FrequencyBand,
        sample_spectrum: impl FnMut(u16) -> Option<f32>,
    ) -> &CacheLine {
        let line_index = usize::from(y);

        // clear cache if view frequency band changed
        if self
            .view_frequency_band
            .map_or(true, |band| band != view_frequency_band)
        {
            self.lines.clear();
            self.view_frequency_band = Some(view_frequency_band);
        }

        // clear cache if canvas width changed
        if self
            .canvas_width
            .map_or(true, |canvas_width| canvas_width != width)
        {
            self.lines.clear();
            self.canvas_width = Some(width);
        }

        // this just makes sure that if we happen to render an older line that somehow
        // (impossible!) doesn't exist yet, we make space for it.
        //
        // haha, I had the comparision the wrong way it it quickly filled all memory :D
        while line_index >= self.lines.len() {
            self.lines.push_back(Default::default());
        }

        let line = &mut self.lines[line_index];
        line.fill(width, sample_spectrum);

        &*line
    }
}

#[derive(derive_more::Debug, Default)]
struct CacheLine {
    #[debug("{:?}", debug_limited(samples))]
    samples: Vec<Option<f32>>,
}

impl CacheLine {
    pub fn fill(&mut self, width: u16, mut sample_spectrum: impl FnMut(u16) -> Option<f32>) {
        if self.samples.is_empty() || self.samples.len() != usize::from(width) {
            self.samples = (0..width).map(|x| sample_spectrum(x)).collect();
        }
    }
}

impl Index<usize> for CacheLine {
    type Output = Option<f32>;

    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        &self.samples[index]
    }
}

impl Index<u16> for CacheLine {
    type Output = Option<f32>;

    #[inline(always)]
    fn index(&self, index: u16) -> &Self::Output {
        &self[usize::from(index)]
    }
}
