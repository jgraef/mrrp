pub mod app;
pub mod args;
pub mod demodulator;
pub mod fft;
pub mod files;
pub mod reader;
pub mod ui;
pub mod util;

use std::{
    fs::{
        File,
        OpenOptions,
    },
    io::BufReader,
};

use clap::Parser;
use color_eyre::eyre::{
    Error,
    bail,
};
use rtlsdr_async::{
    Backend,
    RtlSdr,
    rtl_tcp::client::RtlTcpClient,
};
use tracing_subscriber::EnvFilter;

use crate::{
    app::App,
    args::{
        Args,
        Command,
        MainArgs,
    },
    files::AppFiles,
    ui::bookmarks::import_sdrpp_bookmarks,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    color_eyre::install()?;

    let app_files = AppFiles::new()?;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(
            OpenOptions::new()
                .append(true)
                .create(true)
                .open(app_files.log_file())?,
        )
        .init();

    tracing::info!("Starting mrrp-cli");
    let args = Args::parse();
    tracing::debug!(?args);

    let result = match args.command.unwrap_or_default() {
        Command::Main(args) => {
            async fn run_app<B>(
                args: MainArgs,
                app_files: AppFiles,
                rtl_sdr: B,
            ) -> Result<(), Error>
            where
                B: Backend + Send + Clone + 'static,
                <B as Backend>::Error: std::error::Error + Send + Sync + 'static,
            {
                let mut app = App::new(args, app_files, rtl_sdr).await?;
                app.run().await?;
                app.persist()?;
                Ok(())
            }

            match (&args.device, &args.address) {
                (device_opt, None) => {
                    let rtl_sdr = RtlSdr::open(device_opt.unwrap_or_default())?;
                    run_app(args, app_files, rtl_sdr).await
                }
                (None, Some(address)) => {
                    let rtl_tcp = RtlTcpClient::connect(address).await?;
                    run_app(args, app_files, rtl_tcp).await
                }
                (Some(_), Some(_)) => {
                    bail!("Only either --device or --address can be used at once")
                }
            }
        }
        Command::DumpState { path } => {
            let app_state = if let Some(path) = path {
                ciborium::from_reader(BufReader::new(File::open(path)?))?
            }
            else {
                app_files.load_app_state()?
            };

            println!("{app_state:#?}");
            Ok(())
        }
        Command::ImportSdrppBookmarks(args) => {
            let mut bookmarks = app_files.bookmarks()?;
            for bookmark in import_sdrpp_bookmarks(&args.path)? {
                bookmarks.add_and_save_bookmark(bookmark)?;
            }
            Ok(())
        }
    };

    if let Err(error) = &result {
        tracing::error!(?error);
    }
    else {
        tracing::info!("Program exiting");
    }

    result
}
