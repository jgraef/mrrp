use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::Error;
use mrrp::{
    GetSampleRate,
    filter::{
        biquad,
        design::{
            FilterDesign,
            Lowpass,
            Normalize,
            pm_remez::pm_remez,
        },
    },
    io::AsyncReadSamplesExt,
    modem::fm::FmModulator,
    sink::{
        file::write_stream_to_wav,
        rtl_tcp,
    },
    source::file::WavSource,
};
use tokio::net::TcpListener;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    tracing::info!("FM modulator example");

    let args = Args::parse();

    // Convert from any format to the expected 32-bit float single-channel wav:
    // sox input_file -c 1 -b 32 -e float output.wav

    let wav_source = WavSource::<_, f32>::from_path(&args.input)?;
    let sample_rate = wav_source.sample_rate();

    let interpolated = wav_source
        .interpolate_to(2400000.0)
        .scan_in_place_with(biquad::lowpass(sample_rate, 20000.0));

    let sample_rate = interpolated.sample_rate();
    let fm_modulated = interpolated.scan_with(FmModulator::new(sample_rate, 75000.0));

    //let filtered = fm_modulated;
    let filter_design = pm_remez(
        Lowpass::new(75000.0, 5000.0, 0.05, 0.005).normalize(sample_rate),
        11,
    )
    .unwrap();
    println!("filter design: {filter_design:#?}");

    let filtered = fm_modulated.scan_in_place_with(filter_design.fir_filter());
    println!("output sample rate: {}", filtered.sample_rate());

    if let Some(output) = &args.file_output {
        write_stream_to_wav(output, filtered).await?;
    }
    else if let Some(output) = &args.tcp_output {
        //let tcp_stream = TcpStream::connect(&output).await?;
        println!("Waiting for connection");
        let tcp_listener = TcpListener::bind(&output).await?;
        let (tcp_stream, _) = tcp_listener.accept().await?;
        println!("Client connected");
        rtl_tcp::serve_connection(tcp_stream, filtered.throttle_to_sample_rate()).await?;
    }

    Ok(())
}

#[derive(Debug, clap::Parser)]
struct Args {
    //#[clap(short, long)]
    //sample_rate: f32,
    input: PathBuf,

    #[clap(short = 'o', long)]
    file_output: Option<PathBuf>,

    #[clap(long)]
    tcp_output: Option<String>,
}
