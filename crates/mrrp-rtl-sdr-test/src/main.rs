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
    sync::OnceLock,
    time::{
        Duration,
        Instant,
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
    open::DeviceArgs,
    regdump::{
        dump_regs,
        hexyl,
        print_reg_dump,
    },
    server::{
        ServerConfig,
        ServerHandler,
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
        Command::PoweronDemod { device } => {
            let (mut rtl2832u, _) = device.open_rtl2832u().await?;
            rtl2832u.poweron_demod().await?;
        }
        Command::Reset { device } => {
            let (mut rtl2832u, _) = device.open_rtl2832u().await?;
            rtl2832u.reset(Default::default()).await?;
        }
        Command::DumpRegs {
            device,
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
                // todo: also dump undocumented demod pages
                demod.extend(0..5);
                usb = true;
                system = true;
                // todo: tuner, rom
            }

            dump_regs(device, demod, usb, system, tuner, rom, tuner_i2c, path).await?;
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
            device,
            length,
            output,
        } => {
            // note: doesn't work?

            let mut writer = BufWriter::new(File::create(&output)?);

            let (rtl2832u, _) = device.open_rtl2832u().await?;

            let data = rtl2832u
                .read(reg::Register::Rom { address: 0 }, length)
                .await?;

            writer.write_all(&data)?;
        }
        Command::Gpio {
            device,
            pin,
            command,
        } => {
            gpio_command(device, pin, command).await?;
        }
        Command::BiasTee { device, command } => {
            gpio_command(device, 0, command).await?;
        }
        Command::Test {
            device,
            stream,
            sample_rate,
            center_frequency,
            test_hop,
        } => {
            let mut device = device.open_device().await?;

            tracing::info!(?sample_rate, "Setting sample rate");
            device.set_sample_rate(sample_rate).await?;

            tracing::info!(?center_frequency, "Setting center frequency");
            device.set_center_frequency(center_frequency).await?;

            if stream {
                let mut reader = device.reader(Default::default()).await?;

                struct SamplesPerSecond {
                    start_time: Instant,
                    sample_count: usize,
                }

                let mut samples_per_second: Option<SamplesPerSecond> = None;
                const MEASUREMENT_INVERVAL: Duration = Duration::from_secs(1);

                let stream_task = tokio::spawn(async move {
                    run_til_shutdown(async {
                        loop {
                            match reader.fill_buf().await {
                                Ok(buffer) => {
                                    let buffer_len = buffer.len();
                                    if buffer_len % 2 != 0 {
                                        tracing::warn!(
                                            buffer_len,
                                            "Returned sample buffer's length is not a multiple of 2"
                                        );
                                    }

                                    let now = Instant::now();
                                    let sample_count = buffer.len() / 2;

                                    if let Some(samples_per_second) = &mut samples_per_second {
                                        samples_per_second.sample_count += sample_count;
                                        let interval = now - samples_per_second.start_time;

                                        if interval > MEASUREMENT_INVERVAL {
                                            let sps = samples_per_second.sample_count as f32
                                                / interval.as_secs_f32();
                                            tracing::info!(
                                                "Samples per second: {:.3} MSa/s",
                                                sps / 1000000.0
                                            );

                                            // reset measurement
                                            samples_per_second.start_time = now;
                                            samples_per_second.sample_count = 0;
                                        }
                                    }
                                    else {
                                        samples_per_second = Some(SamplesPerSecond {
                                            start_time: now,
                                            sample_count,
                                        });
                                    }

                                    /*for k in 0..n {
                                        // not exactly right but close enough
                                        let _i = ((buffer[k * 2] as f32) - 128.0) / 255.0;
                                        let _q = ((buffer[k * 2 + 1] as f32) - 128.0) / 255.0;

                                        //println!("{i:.04}+{q:.04}i, ");
                                    }*/

                                    reader.consume(buffer_len);
                                }
                                Err(error) => {
                                    if error.kind() == std::io::ErrorKind::UnexpectedEof {
                                        tracing::warn!("reader eof");
                                    }
                                    else {
                                        tracing::warn!(%error, "reader error");
                                    };

                                    break;
                                }
                            }
                        }
                    })
                    .await;

                    tracing::info!("Closing reader");
                    reader.close().await?;

                    Ok::<(), Error>(())
                });

                if test_hop {
                    // test to change center frequnecy after a while
                    tokio::time::sleep(Duration::from_secs(5)).await;

                    let new_center_frequency = center_frequency + 400_000.0;
                    tracing::info!("Changing center frequency to {new_center_frequency}");

                    let t_start = Instant::now();
                    device.set_center_frequency(new_center_frequency).await?;
                    let dt = t_start.elapsed();

                    tracing::info!("Changing frequency took {dt:?}");
                }

                // wait for stream task to finish
                stream_task.await??;
            }

            tracing::info!("Closing device");
            device.close().await?;
        }
        Command::Tcp {
            device,
            listen_address,
            buffer_size,
            log_dropped,
            fix_tuner_frequency,
        } => {
            let device = device.open_device().await?;
            let tcp_listener = TcpListener::bind(listen_address).await?;
            let server_config = ServerConfig {
                buffer_size,
                fix_tuner_frequency,
            };
            let server_handler = ServerHandler::new(device, server_config)
                .await?
                .with_log_dropped(log_dropped);
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
        Command::SysDump { device, output } => {
            let mut writer = BufWriter::new(File::create(&output)?);

            let (rtl2832u, _) = device.open_rtl2832u().await?;

            let length = 0x1000;

            for i in 0..0x10 {
                let address = i * length;
                tracing::info!(?address, ?length, "dumping");

                match rtl2832u
                    .read(reg::Register::System { address }, length)
                    .await
                {
                    Ok(data) => {
                        hexyl(&data, address.try_into().unwrap());
                        writer.write_all(&data)?;
                    }
                    Err(error) => {
                        tracing::error!(?address, ?length, %error, "dump failed");
                        for _ in 0..length {
                            writer.write_all(&[0])?;
                        }
                    }
                }
            }
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
        #[clap(flatten)]
        device: DeviceArgs,
    },
    /// Reset the device.
    ///
    /// This doesn't perform a soft or hard reset (via the bits), but turns off
    /// the DEMOD chip and disables all GPIO outputs.
    Reset {
        #[clap(flatten)]
        device: DeviceArgs,
    },
    /// Dump registers
    ///
    /// This dumps the registers documented in the datasheet. More specifically
    /// it'll dump the whole blocks that are known to work. It'll always dump
    /// the blocks with their known base addresses.
    DumpRegs {
        #[clap(flatten)]
        device: DeviceArgs,

        /// Demod blocks
        ///
        /// This contains registers for demodulator. The block consists of 5
        /// pages.
        #[clap(long)]
        demod: Vec<u8>,

        /// USB block
        ///
        /// This contains registers for the USB interface.
        #[clap(long)]
        usb: bool,

        /// System block
        ///
        /// This contains registers for the 8051 system processor.
        #[clap(long)]
        system: bool,

        /// This block seems to be unused.
        #[clap(long, hide = true)]
        tuner: bool,

        /// This doesn't seem to work. We also tried to implement a separate
        /// command for this.
        #[clap(long, hide = true)]
        rom: bool,

        /// This dumps the registers of the tuner
        ///
        /// This will access the tuner via I2C and dump the registers. Currently
        /// only support for R82XX is implemented. A quirk of the R82XX are that
        /// you will only get the first 16 of the 32 registers.
        #[clap(long)]
        tuner_i2c: bool,

        /// Specify path for output
        ///
        /// `dump-regs` will always output files with fixed file names, as it'll
        /// usually also generate multiple files. This specifies the directory
        /// into which to write these files. The default directory is the
        /// current directory.
        #[clap(short, long)]
        output: Option<PathBuf>,
    },
    /// Print register dump
    ///
    /// This dumps registers as documented in the datasheet.
    PrintRegDump {
        /// Path to register dump.
        ///
        /// This is usually the directory into which `dump-regs` wrote files.
        path: Option<PathBuf>,

        /// Decode starting from `offset`.
        #[clap(short, long)]
        offset: Option<usize>,

        /// Only decode `length` bytes.
        #[clap(short, long)]
        length: Option<usize>,

        /// Decode meanings of known registers
        #[clap(short = 'd', long)]
        decode: bool,

        /// Print a hex dump
        #[clap(short = 'H', long)]
        hexdump: bool,
    },
    /// Dump ROM code
    ///
    /// Doesn't seem to work.
    #[clap(hide = true)]
    DumpRomCode {
        #[clap(flatten)]
        device: DeviceArgs,

        #[clap(short, long)]
        output: PathBuf,

        #[clap(short, long)]
        length: u16,
    },
    /// Control GPIO pins
    Gpio {
        #[clap(flatten)]
        device: DeviceArgs,

        /// GPIO pin number
        ///
        /// The RTL2832U has pins 0 to 7 (inclusive)
        ///
        /// Be careful! Pin 0 on the RTL-SDR Blog V4 is GPIO. Thus if you have
        /// your antenna shorted, enabling this can cause a short-circuit. Other
        /// SDRs might use these pins in other ways.
        ///
        /// Also on the RTL-SDR Blog V4, the upconverter will be on pin 5.
        pin: u8,

        #[clap(subcommand)]
        command: GpioCommand,
    },
    /// Control Bias-Tee
    ///
    /// This is just an alias for `gpio 0`.
    ///
    /// This works only for the RTL-SDR Blog V4, or other RTL-SDRs that use GPIO
    /// pin 0 for the Bias-Tee.
    ///
    /// Be careul! This will put ~5V on your antenna. Make sure it's not
    /// shorted.
    BiasTee {
        #[clap(flatten)]
        device: DeviceArgs,

        #[clap(subcommand)]
        command: GpioCommand,
    },
    /// Test a device
    ///
    /// Opens the device, configures sample rate and center frequency, and
    /// optionally streams samples from it.
    Test {
        #[clap(flatten)]
        device: DeviceArgs,

        /// Sets the devices sample rate
        #[clap(short = 'S', long, default_value = "2400000")]
        sample_rate: f32,

        /// Sets the device's center frequency
        #[clap(short = 'F', long, default_value = "144000000")]
        center_frequency: f32,

        /// Enables IQ streaming
        ///
        /// Enable this if you want to test if IQ samples can be read from the
        /// device. The program will stream IQ samples until it's interrupted
        /// via Ctrl-C. It will display the measured sample rate while doing so.
        #[clap(short, long)]
        stream: bool,

        /// Performs a change of center frequency after 5 seconds.
        ///
        /// This will tune up by 400 kHz after 5 seconds to test the ability to
        /// change center frequency while the device is in use.
        #[clap(long)]
        test_hop: bool,
    },
    /// rtl_tcp server
    ///
    /// This will run a server compatible with the rtl_tcp protocol. In contrast
    /// to the original rtl_tcp this will accept connections from multiple
    /// clients at once.
    Tcp {
        #[clap(flatten)]
        device: DeviceArgs,

        /// Listen address and port
        #[clap(short, long, default_value = "localhost:1234")]
        listen_address: String,

        /// Buffer size
        ///
        /// The USB interface will use buffers to facility reading samples from
        /// the device and this program also uses a ring buffer to broadcast the
        /// sample sample stream to multiple clients. This configures the buffer
        /// size for both.
        ///
        /// This must be a multiple of 2 and greater than 0. In practice it
        /// should be choosen much larger than that. 64 kB seems to be an
        /// approproate size.
        #[clap(short, long, default_value = "65536")]
        buffer_size: usize,

        /// Logs if samples have been dropped
        ///
        /// This will log warnings if a client is lagging such that some samples
        /// for them have been dropped.
        ///
        /// The ring buffer used for broadcasting to multiple clients will not
        /// wait for all clients to read all available data when it drops old
        /// data from the ring buffer. Thus, if a client is too slow at
        /// receiving the data, some samples for them might get lost.
        ///
        /// Since the rtl_tcp protocol is rather simple, this can't be reported
        /// to the client unfortunately.
        ///
        /// To be able to see the log messages, you need to set the `RUST_LOG`
        /// environment variable, e.g. `RUST_LOG=mrrp=warn`.
        #[clap(long)]
        log_dropped: bool,

        /// Fix tuner frequency
        ///
        /// This feature was implemented for testing purposes. This will fix the
        /// tuner frequency to the provided value. When the client sets a center
        /// frequency, it'll only affect the frequency (relative to IF) at which
        /// the RTL samples.
        ///
        /// This makes it possible to look at the signal received from the tuner
        /// outside of the intended band.
        #[clap(long)]
        fix_tuner_frequency: Option<f32>,
    },
    /// Dumps system memory.
    ///
    /// This reads from the system block, but doesn't respect the
    /// datasheet-specified base address. It reads 0x00000 .. 0x10000 in chunks
    /// of 0x1000.
    SysDump {
        #[clap(flatten)]
        device: DeviceArgs,

        #[clap(short, long)]
        output: PathBuf,
    },
}

fn shutdown_signal() -> CancellationToken {
    static ONCE: OnceLock<CancellationToken> = OnceLock::new();

    ONCE.get_or_init(|| {
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
    })
    .clone()
}

async fn run_til_shutdown<R>(fut: impl Future<Output = R>) -> Option<R> {
    let cancellation_token = shutdown_signal();
    tokio::select! {
        _ = cancellation_token.cancelled() => None,
        output = fut => Some(output),
    }
}
