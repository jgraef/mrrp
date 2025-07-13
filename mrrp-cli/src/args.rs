use std::{
    path::PathBuf,
    str::FromStr,
};

use clap::FromArgMatches;

use crate::{
    Error,
    fft::Window,
};

#[derive(Debug, clap::Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, clap::Subcommand)]
pub enum Command {
    #[clap(name = "tui")]
    Main(MainArgs),
}

impl Default for Command {
    fn default() -> Self {
        Self::Main(
            MainArgs::from_arg_matches(&Default::default())
                .expect("bug: MainArgs wasn't parsed from empty args"),
        )
    }
}

#[derive(Debug, clap::Args)]
pub struct MainArgs {
    /// Device index to use. If neither this or --address is specified, the
    /// first device is used.
    #[clap(short, long)]
    pub device: Option<u32>,

    #[clap(short, long)]
    pub address: Option<String>,

    /// Sample rate. This determines the bandwidth of the spectrum.
    #[clap(short, long = "samplerate", default_value = "2400000")]
    pub sample_rate: u32,

    /// Center frequency
    #[clap(short, long, default_value = "7000000")]
    pub frequency: u32,

    /// Gain
    #[clap(short, long, default_value = "auto")]
    pub gain: Gain,

    /// Scroll down one line every X milliseconds.
    #[clap(long, default_value = "250")]
    pub scroll_interval: u64,

    /// Redraw the screen every X milliseconds.
    #[clap(long, default_value = "100")]
    pub redraw_interval: u64,

    /// Don't load the previous UI state from file.
    #[clap(long)]
    pub reset_ui: bool,

    /// Use the specified file instead of the default bandplan
    #[clap(long)]
    pub bandplan: Option<PathBuf>,

    /// Use the specified file instead of the default keybinds
    pub keybinds: Option<PathBuf>,

    /// Size of segments that are FFT'd
    #[clap(long, default_value = "16384")]
    pub fft_size: usize,

    /// Overlap of segments that are FFT'd
    #[clap(long, default_value = "0")]
    pub fft_overlap: usize,

    #[clap(long, default_value = "boxcar")]
    pub fft_window: Window,
}

#[derive(Clone, Copy, Debug)]
pub enum Gain {
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
