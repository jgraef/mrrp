pub mod app;
pub mod args;
pub mod fft;
pub mod reader;
pub mod ui;
pub mod util;

use std::fs::OpenOptions;

use clap::Parser;
use color_eyre::eyre::{
    Error,
    bail,
};
use crossterm::{
    event::{
        DisableMouseCapture,
        EnableMouseCapture,
    },
    execute,
};
use rtlsdr_async::{
    Backend,
    RtlSdr,
    rtl_tcp::client::RtlTcpClient,
};
use tracing_subscriber::EnvFilter;

use crate::{
    app::App,
    args::Args,
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

    async fn run_app<B>(args: Args, rtl_sdr: B) -> Result<(), Error>
    where
        B: Backend + Send + Clone + 'static,
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
