pub mod fft;
pub mod reader;
pub mod ui;
pub mod util;

use std::{
    fmt::Debug,
    fs::OpenOptions,
    str::FromStr,
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
        EventStream,
    },
    execute,
};
use futures_util::TryStreamExt;
use ratatui::DefaultTerminal;
use rtlsdr_async::{
    Backend,
    RtlSdr,
    rtl_tcp::client::RtlTcpClient,
};
use tracing_subscriber::EnvFilter;

use crate::{
    fft::{
        Fft,
        Window,
    },
    reader::SampleReader,
    ui::{
        Event,
        Ui,
    },
    util::FrequencyBand,
};

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
    tracing::debug!(?args);

    // we need to get this before creating the terminal window, as librtlsdr just
    // prints stuff (how rude!).
    let result = match (&args.device, &args.address) {
        (device_opt, None) => {
            let rtl_sdr = RtlSdr::open(device_opt.unwrap_or_default())?;
            run_app(args, rtl_sdr).await
        }
        (None, Some(address)) => {
            let rtl_tcp = RtlTcpClient::connect(address).await?;
            run_app(args, rtl_tcp).await
        }
        (Some(_), Some(_)) => bail!("Only either --device or --address can be used at once"),
    };

    async fn run_app<B: Backend>(args: Args, rtl_sdr: B) -> Result<(), Error>
    where
        <B as Backend>::Error: std::error::Error + Send + Sync + 'static,
    {
        if args.fft_size == 0 {
            bail!("FFT size must be greater than 0");
        }
        if args.fft_size & 1 == 1 {
            bail!("FFT size must be a multiple of 2")
        }
        if args.fft_overlap >= args.fft_size {
            bail!("FFT overlap must be less than FFT size");
        }
        if args.sample_rate & 1 == 1 {
            // todo: we currently can't calculate the start and end frequency of the signal
            // correctly in this case.
            bail!("Sample rate must be divisble by 2");
        }

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

    /// Gain
    #[clap(short, long, default_value = "auto")]
    gain: Gain,

    /// Scroll down one line every X milliseconds.
    #[clap(long, default_value = "250")]
    scroll_interval: u64,

    /// Size of segments that are FFT'd
    #[clap(long, default_value = "1024")]
    fft_size: usize,

    /// Overlap of segments that are FFT'd
    #[clap(long, default_value = "256")]
    fft_overlap: usize,

    #[clap(long, default_value = "boxcar")]
    fft_window: Window,
}

#[derive(Debug)]
struct App<B> {
    terminal: DefaultTerminal,
    scroll_interval: Duration,

    #[allow(unused)]
    rtl_sdr: B,
    sample_reader: SampleReader,
    fft: Fft,

    ui: Ui,
}

impl<B: Backend> App<B>
where
    <B as Backend>::Error: std::error::Error + Send + Sync + 'static,
{
    async fn new(args: Args, terminal: DefaultTerminal, rtl_sdr: B) -> Result<Self, Error> {
        let half_bandwidth = args.sample_rate / 2;
        let center_frequency = args.frequency.max(half_bandwidth);
        let sampled_frequency_band = FrequencyBand {
            start: args.frequency - half_bandwidth,
            end: args.frequency + half_bandwidth,
        };

        rtl_sdr.set_center_frequency(center_frequency).await?;
        rtl_sdr.set_sample_rate(args.sample_rate).await?;
        rtl_sdr.set_tuner_gain(args.gain.into()).await?;

        let sample_reader =
            SampleReader::new(rtl_sdr.samples().await?, args.fft_size, args.fft_overlap);

        Ok(Self {
            terminal,
            scroll_interval: Duration::from_millis(args.scroll_interval),
            rtl_sdr,
            sample_reader,
            fft: Fft::new(args.fft_size, args.fft_window),
            ui: Ui::new(sampled_frequency_band),
        })
    }

    async fn run(&mut self) -> Result<(), Error> {
        let mut events = EventStream::new();
        let mut scroll_interval = tokio::time::interval(self.scroll_interval);

        while !self.ui.exit_requested() {
            tokio::select! {
                result = events.try_next() => {
                    let Some(event) = result?
                    else {
                        break;
                    };
                    self.ui.handle_event(Event::Terminal(event));
                }
                _ = scroll_interval.tick() => {
                    self.ui.handle_event(Event::ScrollWaterfall);
                    self.terminal.draw(|frame| frame.render_widget(&mut self.ui, frame.area()))?;
                }
                result = self.sample_reader.read() => {
                    let Some(samples) = result?
                    else {
                        tracing::warn!("sample stream stopped");
                        break;
                    };

                    let spectrum = self.fft.forward(samples);
                    self.ui.handle_event(Event::Spectrum { spectrum, });
                }
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum Gain {
    Value(f32),
    Auto,
}

impl From<Gain> for rtlsdr_async::Gain {
    fn from(value: Gain) -> Self {
        match value {
            Gain::Value(gain) => Self::ManualValue((gain * 10.0) as i32),
            Gain::Auto => Self::Auto,
        }
    }
}

impl FromStr for Gain {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto" => Ok(Self::Auto),
            _ => Ok(Self::Value(s.parse()?)),
        }
    }
}
