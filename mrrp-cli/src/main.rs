use std::{
    collections::VecDeque,
    fs::{
        File,
        OpenOptions,
    },
    io::BufWriter,
    time::Duration,
};

use clap::Parser;
use color_eyre::eyre::Error;
use crossterm::event::{
    Event,
    EventStream,
    KeyCode,
};
use futures_util::TryStreamExt;
use mrrp::source::rtlsdr;
use num_complex::Complex;
use ratatui::{
    DefaultTerminal,
    buffer::Buffer,
    layout::{
        Position,
        Rect,
    },
    palette::Hsl,
    style::Color,
    widgets::Widget,
};
use rtlsdr_async::{
    Chunk,
    Gain,
    Iq,
    RtlSdr,
    Samples,
};
use rustfft::FftPlanner;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    //color_eyre::install()?;
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(OpenOptions::new().append(true).open("mrrp-cli.log")?)
        .init();

    tracing::info!("Starting mrrp-cli");
    let args = Args::parse();

    // we need to get this before creating the terminal window, as librtlsdr just
    // prints stuff (how rude!).
    let rtl_sdr = RtlSdr::open(args.device)?;

    let terminal = ratatui::init();
    let result = App::new(args, terminal, rtl_sdr).await?.run().await;
    if let Err(error) = &result {
        tracing::error!(?error);
    }
    else {
        tracing::info!("Program exiting");
    }

    // fixme: terminal scrolling doesn't work when the program exits.
    ratatui::restore();
    result
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(short, long, default_value = "0")]
    device: u32,
    #[clap(short, long = "samplerate", default_value = "2400000")]
    sample_rate: u32,
    #[clap(short, long, default_value = "7000000")]
    frequency: u32,
}

#[derive(Debug)]
struct App {
    terminal: DefaultTerminal,
    redraw_interval: Duration,
    scroll_interval: Duration,
    waterfall: Waterfall,
    exit: bool,
    rtl_sdr: RtlSdr,
    sample_rate: u32,
    sample_reader: SampleReader,
    fft: Fft,
}

impl App {
    async fn new(args: Args, terminal: DefaultTerminal, rtl_sdr: RtlSdr) -> Result<Self, Error> {
        rtl_sdr.set_center_frequency(args.frequency).await?;
        rtl_sdr.set_sample_rate(args.sample_rate).await?;
        rtl_sdr.set_tuner_gain(Gain::Auto).await?;
        let sample_reader = SampleReader::new(rtl_sdr.samples().await?);

        let mut waterfall = Waterfall::default();
        waterfall.min_z = -80.0;
        waterfall.max_z = -70.0;

        Ok(Self {
            terminal,
            redraw_interval: Duration::from_millis(250),
            scroll_interval: Duration::from_millis(500),
            waterfall,
            exit: false,
            rtl_sdr,
            sample_rate: args.sample_rate,
            sample_reader,
            fft: Default::default(),
        })
    }

    async fn run(&mut self) -> Result<(), Error> {
        let mut events = EventStream::new();
        let mut redraw_interval = tokio::time::interval(self.redraw_interval);
        let mut scroll_interval = tokio::time::interval(self.scroll_interval);

        while !self.exit {
            tokio::select! {
                result = events.try_next() => {
                    let Some(event) = result?
                    else {
                        break;
                    };
                    self.handle_event(event).await?;
                }
                _ = redraw_interval.tick() => {
                    self.draw()?;
                }
                _ = scroll_interval.tick() => {
                    self.waterfall.scroll()
                }
                result = self.sample_reader.read(self.waterfall.width), if self.waterfall.width > 0 => {
                    let Some(samples) = result?
                    else {
                        break;
                    };
                    self.waterfall.push(self.fft.spectrum(samples));
                }
            }
        }

        Ok(())
    }

    fn draw(&mut self) -> Result<(), Error> {
        self.waterfall.sample_rate = self.sample_rate as f32;
        self.terminal
            .draw(|frame| frame.render_widget(&mut self.waterfall, frame.area()))?;
        Ok(())
    }

    async fn handle_event(&mut self, event: Event) -> Result<(), Error> {
        match event {
            Event::Key(key_event) => {
                match key_event.code {
                    KeyCode::Char('q') => {
                        self.exit = true;
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Debug, Default)]
struct Waterfall {
    new_line: Option<NewLine>,
    lines: VecDeque<Line>,
    width: usize,
    height: usize,
    min_z: f32,
    max_z: f32,
    sample_rate: f32,
}

impl Waterfall {
    fn scroll(&mut self) {
        if let Some(mut line) = self.new_line.take() {
            // z is the energy for that frequency over line.count * sample_rate. convert to
            // power in dBFS.
            let normalize = 1.0 / (line.count as f32 * self.sample_rate);
            for z in &mut line.fft {
                *z = 10.0 * (*z * normalize).log10();
            }

            while self.lines.len() >= self.height {
                self.lines.pop_front();
            }

            self.lines.push_back(Line { fft: line.fft });
        }
    }

    fn color(&self, z: f32) -> Color {
        let scaled = (z - self.min_z) / (self.max_z - self.min_z);
        let hue = -120.0 * (1.0 - scaled).clamp(0.0, 1.0);
        Color::from_hsl(Hsl::new(hue, 1.0, 0.5))
    }

    fn push(&mut self, spectrum: &[Complex<f32>]) {
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
}

impl Widget for &mut Waterfall {
    fn render(self, area: Rect, buf: &mut Buffer)
    where
        Self: Sized,
    {
        self.width = area.width.into();
        self.height = area.height.into();

        let mut min_max: Option<(f32, f32)> = None;
        for z in self.lines.iter().flat_map(|line| line.fft.iter()) {
            if let Some((min, max)) = &mut min_max {
                *min = min.min(*z);
                *max = max.max(*z);
            }
            else {
                min_max = Some((*z, *z));
            }
        }
        if let Some((min, max)) = min_max {
            self.min_z = min;
            self.max_z = max;
        }

        for y in 0..self.height {
            if let Some(line) = self.lines.len().checked_sub(y + 1).map(|i| &self.lines[i]) {
                for (x, z) in line.fft.iter().enumerate() {
                    let position = Position {
                        x: u16::try_from(x).unwrap() + area.x,
                        y: u16::try_from(y).unwrap() + area.y,
                    };
                    if let Some(cell) = buf.cell_mut(position) {
                        cell.bg = self.color(*z);
                    }
                }
            }
            else {
                for x in 0..self.width {
                    let position = Position {
                        x: u16::try_from(x).unwrap() + area.x,
                        y: u16::try_from(y).unwrap() + area.y,
                    };
                    buf[position].bg = Color::Black;
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
}

#[derive(derive_more::Debug)]
struct Fft {
    #[debug(skip)]
    fft_planner: FftPlanner<f32>,
    fft_buffer: Vec<Complex<f32>>,
    fft_scratch: Vec<Complex<f32>>,
}

impl Default for Fft {
    fn default() -> Self {
        Self {
            fft_planner: FftPlanner::new(),
            fft_buffer: vec![],
            fft_scratch: vec![],
        }
    }
}

impl Fft {
    fn spectrum(&mut self, samples: &[Complex<f32>]) -> &[Complex<f32>] {
        let fft = self.fft_planner.plan_fft_forward(samples.len());

        self.fft_buffer.resize(samples.len(), Default::default());
        self.fft_scratch
            .resize(fft.get_immutable_scratch_len(), Default::default());

        fft.process_immutable_with_scratch(samples, &mut self.fft_buffer, &mut self.fft_scratch);

        // the fft output needs to be normalized with 1/sqrt(n)
        let normalization = 1.0 / (samples.len() as f32).sqrt();
        for x in &mut self.fft_buffer {
            *x *= normalization;
        }

        &self.fft_buffer
    }
}

#[derive(Debug)]
struct SampleReader {
    samples: Samples<Iq>,
    chunk: Option<Chunk<Iq>>,
    read_pos: usize,
    buffer: Vec<Complex<f32>>,
    write_pos: usize,
}

impl SampleReader {
    pub fn new(samples: Samples<Iq>) -> Self {
        Self {
            samples,
            chunk: None,
            read_pos: 0,
            buffer: vec![],
            write_pos: 0,
        }
    }

    pub async fn read(&mut self, num_samples: usize) -> Result<Option<&'_ [Complex<f32>]>, Error> {
        self.buffer.resize(num_samples, Default::default());

        while self.write_pos < num_samples {
            if let Some(chunk) = &self.chunk {
                let samples = chunk.samples();
                while self.write_pos < num_samples && self.read_pos < samples.len() {
                    self.buffer[self.write_pos] = rtlsdr::convert_iq(samples[self.read_pos]);
                    self.write_pos += 1;
                    self.read_pos += 1;
                }

                if self.read_pos >= samples.len() {
                    self.chunk = None;
                    self.read_pos = 0;
                }
            }
            else {
                self.chunk = self.samples.try_next().await?;
                if self.chunk.is_none() {
                    return Ok(None);
                }
            }
        }

        self.write_pos = 0;

        Ok(Some(&self.buffer))
    }
}
