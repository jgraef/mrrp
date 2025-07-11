pub mod waterfall;

use std::{
    fs::OpenOptions,
    time::Duration,
};

use clap::Parser;
use color_eyre::eyre::{
    Error,
    bail,
};
use crossterm::{
    event::{
        DisableMouseCapture,
        EnableMouseCapture,
        Event,
        EventStream,
        KeyCode,
        MouseEventKind,
    },
    execute,
};
use futures_util::TryStreamExt;
use num_complex::Complex;
use ratatui::{
    DefaultTerminal,
    layout::Position,
};
use rtlsdr_async::{
    Backend,
    Chunk,
    Gain,
    Iq,
    RtlSdr,
    Samples,
    rtl_tcp::client::RtlTcpClient,
};
use rustfft::FftPlanner;
use tracing_subscriber::EnvFilter;

use crate::waterfall::Waterfall;

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
    let result = match (&args.device, &args.address) {
        (device_opt, None) => run_app(&args, RtlSdr::open(device_opt.unwrap_or_default())?).await,
        (None, Some(address)) => run_app(&args, RtlTcpClient::connect(address).await?).await,
        (Some(_), Some(_)) => bail!("Only either --device or --address can be used at once"),
    };

    async fn run_app<B: Backend>(args: &Args, rtl_sdr: B) -> Result<(), Error>
    where
        <B as Backend>::Error: std::error::Error + Send + Sync + 'static,
    {
        let terminal = ratatui::init();
        execute!(std::io::stdout(), EnableMouseCapture)?;

        let result = App::new(args, terminal, rtl_sdr).await?.run().await;

        // fixme: terminal scrolling doesn't work when the program exits.
        execute!(std::io::stdout(), DisableMouseCapture)?;
        ratatui::restore();

        result
    }

    if let Err(error) = &result {
        tracing::error!(?error);
    }
    else {
        tracing::info!("Program exiting");
    }

    result
}

#[derive(Debug, Parser)]
struct Args {
    /// Device index to use. If neither this or --address is specified, the
    /// first device is used.
    #[clap(short, long)]
    device: Option<u32>,

    #[clap(short, long)]
    address: Option<String>,

    /// Sample rate. This determines the bandwidth of the spectrum.
    #[clap(short, long = "samplerate", default_value = "2400000")]
    sample_rate: u32,

    /// Center frequency
    #[clap(short, long, default_value = "7000000")]
    frequency: u32,

    /// Scroll down one line every X milliseconds.
    #[clap(short, long, default_value = "250")]
    scroll_interval: u64,
}

#[derive(Debug)]
struct App<B> {
    terminal: DefaultTerminal,
    scroll_interval: Duration,
    waterfall: Waterfall,
    exit: bool,
    #[allow(unused)]
    rtl_sdr: B,
    sample_reader: SampleReader,
    fft: Fft,
    mouse_position: Option<Position>,
}

impl<B: Backend> App<B>
where
    <B as Backend>::Error: std::error::Error + Send + Sync + 'static,
{
    async fn new(args: &Args, terminal: DefaultTerminal, rtl_sdr: B) -> Result<Self, Error> {
        rtl_sdr.set_center_frequency(args.frequency).await?;
        rtl_sdr.set_sample_rate(args.sample_rate).await?;
        rtl_sdr.set_tuner_gain(Gain::Auto).await?;
        let sample_reader = SampleReader::new(rtl_sdr.samples().await?);

        let waterfall =
            Waterfall::new(-80.0, -70.0, args.sample_rate as f32, args.frequency as f32);

        Ok(Self {
            terminal,
            scroll_interval: Duration::from_millis(args.scroll_interval),
            waterfall,
            exit: false,
            rtl_sdr,
            sample_reader,
            fft: Default::default(),
            mouse_position: None,
        })
    }

    async fn run(&mut self) -> Result<(), Error> {
        let mut events = EventStream::new();
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
                _ = scroll_interval.tick() => {
                    self.waterfall.scroll();
                    self.draw()?;
                }
                result = self.sample_reader.read(self.waterfall.width()), if self.waterfall.width() > 0 => {
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
        self.terminal.draw(|frame| {
            frame.render_widget(self.waterfall.widget(self.mouse_position), frame.area())
        })?;
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
            Event::Mouse(mouse_event) => {
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
            Event::FocusLost => {
                self.mouse_position = None;
            }
            _ => {}
        }

        Ok(())
    }
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
                    self.buffer[self.write_pos] = samples[self.read_pos].into();
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
