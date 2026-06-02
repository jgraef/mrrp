pub mod demod_regs;

use std::{
    borrow::Cow,
    fs::File,
    io::{
        BufWriter,
        Cursor,
        Write,
        stdout,
    },
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::{
    Error,
    anyhow,
    bail,
};
use clap::{
    Parser,
    Subcommand,
};
use dotenvy::dotenv;
use mrrp_rtl_sdr::{
    Device,
    enumerate::DeviceInfo,
    rtl2832u::{
        Rtl2832u,
        register::{
            self as reg,
        },
    },
};

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
        Command::PoweronDemod { serial } => {
            let mut rtl2832u = open_rtl2832u(serial.as_deref()).await?;
            rtl2832u.poweron_demod().await?;
        }
        Command::Reset { serial } => {
            let mut rtl2832u = open_rtl2832u(serial.as_deref()).await?;
            rtl2832u.reset().await?;
        }
        Command::ParseDemodRegs => {
            demod_regs::demod_regs();
        }
        Command::DumpRegs {
            serial,
            mut demod,
            mut usb,
            mut system,
            tuner,
            rom,
            output,
        } => {
            let path = output.as_deref().unwrap_or_else(|| Path::new("."));

            if !path.exists() {
                bail!("Directory does not exist: {path:?}");
            }

            if !path.is_dir() {
                bail!("Must be a directory: {path:?}");
            }

            if demod.is_empty() && !usb && !system && !tuner && !rom {
                // all
                demod.extend(0..5);
                usb = true;
                system = true;
                // todo: tuner, rom
            }

            dump_regs(serial.as_deref(), demod, usb, system, tuner, rom, path).await?;
        }
        Command::PrintRegDump {
            path,
            offset,
            length,
            mut decode,
            hexdump,
        } => {
            let path = path.as_deref().unwrap_or_else(|| Path::new("."));

            if !decode && !hexdump {
                decode = true;
            }

            print_reg_dump(path, offset, length, decode, hexdump)?;
        }
        Command::DumpRomCode {
            serial,
            length,
            output,
        } => {
            // note: doesn't work?

            let mut writer = BufWriter::new(File::create(&output)?);

            let mut rtl2832u = open_rtl2832u(serial.as_deref()).await?;

            let data = rtl2832u
                .read(reg::Register::Rom { address: 0 }, length)
                .await?;

            writer.write_all(&data)?;
        }
        Command::Gpio {
            serial,
            pin,
            command,
        } => {
            gpio_command(serial.as_deref(), pin, command).await?;
        }
        Command::BiasTee { serial, command } => {
            gpio_command(serial.as_deref(), 0, command).await?;
        }
        Command::Test { serial } => {
            let device = open_device(serial.as_deref()).await?;
            ////tracing::debug!(tuner = ?device.tuner());

            device.close().await?;
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
    /// List available devices
    List,
    /// Power-on the DEMOD chip.
    ///
    /// This can be useful for e.g. dumping its registers
    PoweronDemod {
        #[clap(short, long)]
        serial: Option<String>,
    },
    /// Reset
    ///
    /// This doesn't perform a soft or hard reset (via the bits), but turns off
    /// the DEMOD chip and disables all GPIO outputs.
    Reset {
        #[clap(short, long)]
        serial: Option<String>,
    },
    #[clap(hide = true)]
    ParseDemodRegs,
    /// Dump registers
    DumpRegs {
        #[clap(short, long)]
        serial: Option<String>,

        #[clap(long)]
        demod: Vec<u8>,

        #[clap(long)]
        usb: bool,

        #[clap(long)]
        system: bool,

        #[clap(long)]
        tuner: bool,

        #[clap(long)]
        rom: bool,

        #[clap(short, long)]
        output: Option<PathBuf>,
    },
    /// Print register dump
    PrintRegDump {
        path: Option<PathBuf>,
        #[clap(short, long)]
        offset: Option<usize>,
        #[clap(short, long)]
        length: Option<usize>,
        #[clap(short = 'd', long)]
        decode: bool,
        #[clap(short = 'H', long)]
        hexdump: bool,
    },
    /// Dump ROM code
    ///
    /// Doesn't seem to work.
    DumpRomCode {
        #[clap(short, long)]
        serial: Option<String>,

        #[clap(short, long)]
        output: PathBuf,

        #[clap(short, long)]
        length: u16,
    },
    /// Control GPIO pins
    Gpio {
        #[clap(short, long)]
        serial: Option<String>,

        pin: u8,

        #[clap(subcommand)]
        command: GpioCommand,
    },
    /// Control Bias-Tee
    ///
    /// This is just an alias for `gpio 0`.
    BiasTee {
        #[clap(short, long)]
        serial: Option<String>,

        #[clap(subcommand)]
        command: GpioCommand,
    },
    Test {
        #[clap(short, long)]
        serial: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
enum GpioCommand {
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

async fn gpio_command(serial: Option<&str>, pin: u8, command: GpioCommand) -> Result<(), Error> {
    let mut rtl2832u = open_rtl2832u(serial).await?;

    match command {
        GpioCommand::Mode => {
            let mode = rtl2832u.gp_direction(pin).await?;
            println!("{mode:?}");
        }
        GpioCommand::Read { output_state } => {
            let state = if output_state {
                let mut pin = rtl2832u.gp_output(pin).await?;
                pin.get_state().await?
            }
            else {
                let mut pin = rtl2832u.gp_input(pin).await?;
                pin.read().await?
            };
            println!("{state:?}");
        }
        GpioCommand::Write { value } => {
            let mut pin = rtl2832u.gp_output(pin).await?;
            pin.write(value).await?;
        }
    }

    Ok(())
}

async fn find_device(serial: Option<&str>) -> Result<DeviceInfo, Error> {
    for device_info in mrrp_rtl_sdr::enumerate_devices().await? {
        if serial.is_none() || device_info.serial_number() == serial {
            tracing::debug!(?device_info, "device found");
            return Ok(device_info);
        }
    }

    if let Some(serial) = serial {
        Err(anyhow!("Device not found: {serial}"))
    }
    else {
        Err(anyhow!("No device found"))
    }
}

async fn open_rtl2832u(serial: Option<&str>) -> Result<Rtl2832u, Error> {
    Ok(find_device(serial)
        .await?
        .open_rtl2832u(Default::default())
        .await?)
}

async fn open_device(serial: Option<&str>) -> Result<Device, Error> {
    Ok(find_device(serial).await?.open(Default::default()).await?)
}

fn reg_dump_file_name_for_block(base: impl AsRef<Path>, block: reg::Block) -> PathBuf {
    let file_name = match block {
        reg::Block::Demod { page } => Cow::Owned(format!("demod_{page}.dat")),
        reg::Block::Usb => Cow::Borrowed("usb.dat"),
        reg::Block::System => Cow::Borrowed("system.dat"),
        reg::Block::Tuner => Cow::Borrowed("tuner.dat"),
        reg::Block::Rom => Cow::Borrowed("rom.dat"),
        reg::Block::I2c => Cow::Borrowed("i2c.dat"),
    };

    base.as_ref().join(&*file_name)
}

fn block_size(block: reg::Block) -> u16 {
    match block {
        reg::Block::Demod { page: _ } => 0x100,
        reg::Block::Usb => 0x1000,
        reg::Block::System => 0x1000,
        reg::Block::Tuner => todo!(),
        reg::Block::Rom => todo!(),
        reg::Block::I2c => todo!(),
    }
}

async fn dump_regs(
    serial: Option<&str>,
    demod: Vec<u8>,
    usb: bool,
    system: bool,
    tuner: bool,
    rom: bool,
    path: impl AsRef<Path>,
) -> Result<(), Error> {
    let mut rtl2832u = open_rtl2832u(serial).await?;

    let mut dump_block = async |block: reg::Block| {
        let base_address = block.base_address().unwrap_or_default();

        tracing::info!(?block, base_address, "Dumping");

        match rtl2832u
            .read(block.with_address(base_address), block_size(block))
            .await
        {
            Ok(data) => {
                reg::demod::visit(PrintRegs {
                    buffer: &data,
                    offset: 0,
                    block,
                });

                std::fs::write(reg_dump_file_name_for_block(&path, block), &data)?;
            }
            Err(error) => {
                tracing::warn!(?block, %error, "Failed to dump block");
            }
        }

        Ok::<(), Error>(())
    };

    for page in demod {
        if page > 4 {
            bail!("Invalid demod page: {page}");
        }
        dump_block(reg::Block::Demod { page }).await?;
    }

    if usb {
        dump_block(reg::Block::Usb).await?;
    }

    if system {
        dump_block(reg::Block::System).await?;
    }

    if tuner {
        todo!();
    }

    if rom {
        todo!();
    }

    Ok(())
}

fn print_reg_dump(
    path: impl AsRef<Path>,
    offset: Option<usize>,
    length: Option<usize>,
    decode: bool,
    hexdump: bool,
) -> Result<(), Error> {
    let print_block = |block: reg::Block| {
        let path = reg_dump_file_name_for_block(&path, block);

        match std::fs::read(&path) {
            Ok(data) => {
                println!("# `{block:?}`\n\n");
                let mut data = &*data;

                if let Some(offset) = offset {
                    data = &data[offset..];
                }
                if let Some(length) = length {
                    data = &data[..length];
                }

                if hexdump {
                    println!("```");
                    hexyl(&data, offset.unwrap_or_default());
                    println!("```\n");
                }
                if decode {
                    println!("```");
                    reg::visit(PrintRegs {
                        buffer: &data,
                        offset: offset.unwrap_or_default(),
                        block,
                    });
                    println!("```\n");
                }
            }
            Err(error) => {
                tracing::warn!(?block, ?path, %error, "Could not read file");
            }
        }
    };

    for page in 0..5 {
        print_block(reg::Block::Demod { page });
    }

    print_block(reg::Block::Usb);
    print_block(reg::Block::System);

    Ok(())
}

pub struct PrintRegs<'a> {
    buffer: &'a [u8],
    offset: usize,
    block: reg::Block,
}

impl<'a> reg::Visitor for PrintRegs<'a> {
    fn visit<R>(&mut self)
    where
        R: reg::RegisterValue,
    {
        if self.block == R::ADDRESS.block() {
            let offset = usize::try_from(
                R::ADDRESS.address() - self.block.base_address().unwrap_or_default(),
            )
            .unwrap();

            if let Some(offset) = offset.checked_sub(self.offset) {
                let n = usize::try_from(<R::Bits as reg::Bits>::LENGTH).unwrap();
                if offset + n <= self.buffer.len() {
                    let data = &self.buffer[offset..][..n];
                    let bits = <R::Bits as reg::Bits>::from_bytes(data);
                    let value = R::from_bits(bits);
                    println!("{:?} = {value:?}", R::ADDRESS);
                }
            }
        }
    }
}

fn hexyl(data: &[u8], offset: usize) {
    let mut stdout = stdout();
    let mut printer = hexyl::PrinterBuilder::new(&mut stdout).build();
    printer.display_offset(offset.try_into().unwrap());
    printer.print_all(Cursor::new(data)).unwrap();
}
