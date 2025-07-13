use std::str::FromStr;

use clap::Parser;

use crate::{
    Error,
    fft::Window,
};

#[derive(Debug, Parser)]
pub struct Args {
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

    /// Size of segments that are FFT'd
    #[clap(long, default_value = "16384")]
    pub fft_size: usize,

    /// Overlap of segments that are FFT'd
    #[clap(long, default_value = "0")]
    pub fft_overlap: usize,

    #[clap(long, default_value = "hann")]
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
