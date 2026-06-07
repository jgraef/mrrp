use std::{
    borrow::Cow,
    io::{
        Cursor,
        stdout,
    },
    path::{
        Path,
        PathBuf,
    },
};

use anyhow::{
    Error,
    bail,
};
use mrrp_rtl_sdr::{
    rtl2832u::register as reg,
    tuner::{
        Tuner,
        TunerProbe,
        r82xx,
    },
};

use crate::open::open_rtl2832u;

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

pub async fn dump_regs(
    serial: Option<&str>,
    demod: Vec<u8>,
    usb: bool,
    system: bool,
    tuner: bool,
    rom: bool,
    tuner_i2c: bool,
    path: impl AsRef<Path>,
) -> Result<(), Error> {
    let path = path.as_ref();
    let mut rtl2832u = open_rtl2832u(serial).await?;

    if !demod.is_empty() || tuner_i2c {
        tracing::info!("We have to poweron the DEMOD chip.");
        rtl2832u.poweron_demod().await?;
    }

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

    if tuner_i2c {
        let mut i2c_repeater_guard = rtl2832u.enable_i2c_repeater().await?;

        if let Some(tuner) = r82xx::R82xxProbe.try_open(&mut *i2c_repeater_guard).await? {
            let name = tuner.name().to_owned();
            tracing::info!("Found tuner: {name}");

            //let data = tuner.read_registers(0.into(), 0x10).await?;
            //let data2 = tuner.read_registers(0x10.into(), 0x10).await?;

            //hexyl(&data, 0);
            //hexyl(&data2, 0x10);

            //std::fs::write(path.join(format!("tuner_i2c_{name}.dat")),
            // &data)?;
        }
        else {
            tracing::warn!("No tuner found");
        }

        i2c_repeater_guard.disable().await?;
    }

    Ok(())
}

pub fn print_reg_dump(
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
