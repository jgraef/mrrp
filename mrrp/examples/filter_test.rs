use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::Error;
use mrrp::{
    filter::biquad,
    io::AsyncReadSamplesExt,
    sink::{
        file::write_stream_to_wav,
        rtl_tcp,
    },
    source::white_noise,
};
use num_complex::Complex;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    tracing::info!("FM modulator example");

    let args = Args::parse();

    let noise = white_noise::<Complex<f32>>().with_sample_rate(args.sample_rate);

    let cutoff_frequency = args
        .cutoff_frequency
        .unwrap_or_else(|| args.sample_rate / 4.0);
    let output = noise.scan_in_place_with(biquad::lowpass(args.sample_rate, cutoff_frequency));

    if let Some(path) = &args.file_output {
        write_stream_to_wav(path, output).await?;
    }
    else if let Some(address) = &args.tcp_output {
        //let tcp_stream = TcpStream::connect(&output).await?;
        println!("Waiting for connection");
        let tcp_listener = TcpListener::bind(&address).await?;
        let (tcp_stream, _) = tcp_listener.accept().await?;
        println!("Client connected");
        rtl_tcp::serve_connection(tcp_stream, output.throttle_to_sample_rate()).await?;
    }

    Ok(())
}

#[derive(Debug, clap::Parser)]
struct Args {
    #[clap(short, long, default_value = "2400000")]
    sample_rate: f32,

    #[clap(short = 'f', long)]
    cutoff_frequency: Option<f32>,

    #[clap(short = 'o', long)]
    file_output: Option<PathBuf>,

    #[clap(long)]
    tcp_output: Option<String>,
}
