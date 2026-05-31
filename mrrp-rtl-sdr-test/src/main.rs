pub mod demod_regs;

use anyhow::Error;
use clap::{
    Parser,
    Subcommand,
};
use dotenvy::dotenv;
use mrrp_rtl_sdr::Device;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenv();
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    match args.command {
        Command::List => {
            for device_info in mrrp_rtl_sdr::enumerate_devices().await? {
                println!("{device_info:#?}");
            }
        }
        Command::Open => {
            let mut device = open_first().await?;
            let rtl2832u = device.rtl2832u();
            rtl2832u.initialize_baseband().await?;
        }
        Command::Reset => {
            let mut device = open_first().await?;
            let rtl2832u = device.rtl2832u();
            rtl2832u.reset().await?;
        }
        Command::ParseDemodRegs => {
            demod_regs::demod_regs();
        }
    }

    Ok(())
}

#[derive(Debug, Parser)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    List,
    Open,
    Reset,
    ParseDemodRegs,
}

async fn open_first() -> Result<Device, Error> {
    let device = mrrp_rtl_sdr::open_first(Default::default()).await?;
    tracing::debug!(device_info = ?device.device_info(), "device found");
    Ok(device)
}
