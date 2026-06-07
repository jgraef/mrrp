#![feature(vec_deque_truncate_front)]

pub mod gpio;
pub mod open;
pub mod regdump;
pub mod server;

use std::{
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
    bail,
};
use clap::{
    Parser,
    Subcommand,
};
use dotenvy::dotenv;
use futures_util::TryFutureExt;
use mrrp_rtl_sdr::rtl2832u::register::{
    self as reg,
};
use mrrp_rtl_tcp::server::RtlTcpServer;
use tokio::{
    io::AsyncBufReadExt,
    net::TcpListener,
};
use tokio_util::sync::CancellationToken;

use crate::{
    gpio::{
        GpioCommand,
        gpio_command,
    },
    open::{
        open_device,
        open_rtl2832u,
    },
    regdump::{
        dump_regs,
        print_reg_dump,
    },
    server::ServerHandler,
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
            rtl2832u.stop_data_stream().await?;
            rtl2832u.reset(Default::default()).await?;
        }
        Command::DumpRegs {
            serial,
            mut demod,
            mut usb,
            mut system,
            tuner,
            rom,
            tuner_i2c,
            output,
        } => {
            let path = output.as_deref().unwrap_or_else(|| Path::new("."));

            if !path.exists() {
                bail!("Directory does not exist: {path:?}");
            }

            if !path.is_dir() {
                bail!("Must be a directory: {path:?}");
            }

            if demod.is_empty() && !usb && !system && !tuner && !rom && !tuner_i2c {
                // all
                demod.extend(0..5);
                usb = true;
                system = true;
                // todo: tuner, rom
            }

            dump_regs(
                serial.as_deref(),
                demod,
                usb,
                system,
                tuner,
                rom,
                tuner_i2c,
                path,
            )
            .await?;
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
        Command::Test { serial, stream } => {
            let mut device = open_device(serial.as_deref()).await?;

            if stream {
                let mut reader = device.reader(0x100000).await?;

                run_til_shutdown(async {
                    loop {
                        match reader.fill_buf().await {
                            Ok(buffer) => {
                                let n = buffer.len() / 2;
                                for k in 0..n {
                                    // not exactly right but close enough
                                    let _i = ((buffer[k * 2] as f32) - 128.0) / 255.0;
                                    let _q = ((buffer[k * 2 + 1] as f32) - 128.0) / 255.0;

                                    //println!("{i:.04}+{q:.04}i, ");
                                }
                            }
                            Err(error) => {
                                return if error.kind() == std::io::ErrorKind::UnexpectedEof {
                                    tracing::warn!("reader eof");
                                }
                                else {
                                    tracing::warn!(%error, "reader error");
                                };
                            }
                        }
                    }
                })
                .await;
            }

            device.close().await?;
        }
        Command::Tcp {
            serial,
            listen_address,
            buffer_size,
        } => {
            let device = open_device(serial.as_deref()).await?;
            let tcp_listener = TcpListener::bind(listen_address).await?;
            let server_handler = ServerHandler::new(device, buffer_size).await?;
            let server = RtlTcpServer::new(server_handler, tcp_listener)
                .with_graceful_shutdown(shutdown_signal());

            server
                .serve()
                .map_err(|error| {
                    // this needs some explicit conversion because anyhow::Error doesn't implement
                    // std::error::Error. we just convert the non-anyhow variants into anyhow
                    // errors.
                    match error {
                        mrrp_rtl_tcp::server::Error::Socket(error) => error.into(),
                        mrrp_rtl_tcp::server::Error::Handler(error) => error,
                        mrrp_rtl_tcp::server::Error::InvalidCommand(invalid_command) => {
                            invalid_command.into()
                        }
                    }
                })
                .await?;
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

        #[clap(long)]
        tuner_i2c: bool,

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

        #[clap(short, long)]
        stream: bool,
    },
    Tcp {
        #[clap(short, long)]
        serial: Option<String>,

        #[clap(short, long, default_value = "localhost:1234")]
        listen_address: String,

        #[clap(short, long, default_value = "65536")]
        buffer_size: usize,
    },
}

fn shutdown_signal() -> CancellationToken {
    let cancellation_token = CancellationToken::new();

    // todo: sigterm, etc.

    tokio::spawn({
        let cancellation_token = cancellation_token.clone();
        async move {
            if let Err(error) = tokio::signal::ctrl_c().await {
                tracing::error!(%error, "Ctrl-C signal returned an error");
            }

            tracing::info!("Received Ctrl-C. Shutting down.");
            cancellation_token.cancel();
        }
    });

    cancellation_token
}

async fn run_til_shutdown<R>(fut: impl Future<Output = R>) -> Option<R> {
    let cancellation_token = shutdown_signal();
    tokio::select! {
        _ = cancellation_token.cancelled() => None,
        output = fut => Some(output),
    }
}
