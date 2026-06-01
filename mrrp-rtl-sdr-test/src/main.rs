pub mod demod_regs;

use std::{
    borrow::Cow,
    fs::File,
    io::{
        BufWriter,
        Write,
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
    rtl2832u::register::{
        self as reg,
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
        Command::Open { serial } => {
            let mut device = open_device(serial.as_deref()).await?;
            let rtl2832u = device.rtl2832u();

            rtl2832u.initialize_baseband().await?;
        }
        Command::Reset { serial } => {
            let mut device = open_device(serial.as_deref()).await?;
            let rtl2832u = device.rtl2832u();
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
        Command::PrintRegDump { path } => {
            let path = path.as_deref().unwrap_or_else(|| Path::new("."));

            print_reg_dump(path)?;
        }
        Command::DumpRomCode {
            serial,
            length,
            output,
        } => {
            // note: doesn't work?

            let mut writer = BufWriter::new(File::create(&output)?);

            let mut device = open_device(serial.as_deref()).await?;
            let rtl2832u = device.rtl2832u();

            let data = rtl2832u
                .read(reg::Register::Rom { address: 0 }, length)
                .await?;

            writer.write_all(&data)?;
        }
        Command::I2cProbe {
            serial,
            first,
            last,
        } => {
            //let mut device = open_device(serial.as_deref()).await?;
            //let rtl2832u = device.rtl2832u();

            todo!();
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
    Open {
        #[clap(short, long)]
        serial: Option<String>,
    },
    Reset {
        #[clap(short, long)]
        serial: Option<String>,
    },
    ParseDemodRegs,
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
    PrintRegDump {
        path: Option<PathBuf>,
    },
    DumpRomCode {
        #[clap(short, long)]
        serial: Option<String>,

        #[clap(short, long)]
        output: PathBuf,

        #[clap(short, long)]
        length: u16,
    },
    I2cProbe {
        #[clap(short, long)]
        serial: Option<String>,

        first: Option<u16>,

        last: Option<u16>,
    },
}

async fn open_device(serial: Option<&str>) -> Result<Device, Error> {
    for device_info in mrrp_rtl_sdr::enumerate_devices().await? {
        if serial.is_none() || device_info.serial_number() == serial {
            tracing::debug!(?device_info, "device found");

            let device = device_info.open(Default::default()).await?;

            return Ok(device);
        }
    }

    if let Some(serial) = serial {
        Err(anyhow!("Device not found: {serial}"))
    }
    else {
        Err(anyhow!("No device found"))
    }
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
    let mut device = open_device(serial).await?;
    let rtl2832u = device.rtl2832u();

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

fn print_reg_dump(path: impl AsRef<Path>) -> Result<(), Error> {
    let print_block = |block: reg::Block| {
        let path = reg_dump_file_name_for_block(&path, block);

        match std::fs::read(&path) {
            Ok(data) => {
                println!("# `{block:?}`\n\n```");
                reg::visit(PrintRegs {
                    buffer: &data,
                    block,
                });
                println!("```\n");
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

            let n = usize::try_from(<R::Bits as reg::Bits>::LENGTH).unwrap();
            let data = &self.buffer[offset..][..n];
            let bits = <R::Bits as reg::Bits>::from_bytes(data);
            let value = R::from_bits(bits);
            println!("{:?} = {value:?}", R::ADDRESS);
        }
    }
}
