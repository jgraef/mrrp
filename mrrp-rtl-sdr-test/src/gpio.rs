use anyhow::Error;
use clap::Subcommand;

use crate::open::open_rtl2832u;

#[derive(Debug, Subcommand)]
pub enum GpioCommand {
    Mode,
    Read {
        #[clap(short, long)]
        output_state: bool,
    },
    Write {
        // we can't use bool directly here or clap assumes this is a flag.
        #[arg(value_parser = clap::builder::BoolishValueParser::new())]
        value: GpioValue,
    },
}

type GpioValue = bool;

pub async fn gpio_command(
    serial: Option<&str>,
    pin: u8,
    command: GpioCommand,
) -> Result<(), Error> {
    let mut rtl2832u = open_rtl2832u(serial).await?;

    match command {
        GpioCommand::Mode => {
            let mut pin = rtl2832u.gpio(pin);
            let direction = pin.direction().await?;
            let pad_config = pin.pad_config().await?;
            println!("Direction:  {direction:?}");
            println!("PAD config: {pad_config:?}");
        }
        GpioCommand::Read { output_state } => {
            let state = if output_state {
                let mut pin = rtl2832u.gpio(pin).into_output().await?;
                pin.get_state().await?
            }
            else {
                let mut pin = rtl2832u.gpio(pin).into_input().await?;
                pin.read().await?
            };
            println!("{state:?}");
        }
        GpioCommand::Write { value } => {
            let mut pin = rtl2832u.gpio(pin).into_output().await?;
            pin.write(value).await?;
        }
    }

    Ok(())
}
