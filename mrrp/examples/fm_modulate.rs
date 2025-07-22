use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::Error;
use mrrp::{
    GetSampleRate,
    filter::biquad,
    io::AsyncReadSamplesExt,
    sink::{
        file::write_stream_to_wav,
        rtl_tcp,
    },
    source::file::WavSource,
};
use tokio::net::TcpListener;

use crate::fm_modulate::FmModulator;

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

    //let filtered_output = BiquadDf2t::lowpass(fm_modulated, 100000.0);

    if let Some(output) = &args.file_output {
        write_stream_to_wav(output, fm_modulated).await?;
    }
    else if let Some(output) = &args.tcp_output {
        //let tcp_stream = TcpStream::connect(&output).await?;
        println!("Waiting for connection");
        let tcp_listener = TcpListener::bind(&output).await?;
        let (tcp_stream, _) = tcp_listener.accept().await?;
        println!("Client connected");
        rtl_tcp::serve_connection(tcp_stream, fm_modulated.throttle_to_sample_rate()).await?;
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

mod fm_modulate {
    use std::f32::consts::TAU;

    use mrrp::io::Scanner;
    use num_complex::Complex;

    #[derive(Clone, Copy, Debug)]
    pub struct FmModulator {
        delay: f32,
        frequency_modulation_factor: f32,
        carrier_frequency: f32,
    }

    impl FmModulator {
        pub fn new(sample_rate: f32, frequency_deviation: f32) -> Self {
            Self {
                delay: 0.0,
                frequency_modulation_factor: sample_rate / (TAU * frequency_deviation),
                carrier_frequency: 0.0,
            }
        }
    }

    impl Scanner<f32> for FmModulator {
        type Output = Complex<f32>;

        fn scan(&mut self, sample: f32) -> Self::Output {
            let f = self.delay + self.frequency_modulation_factor * sample;
            self.delay = f;
            Complex {
                re: 0.0,
                im: f + self.carrier_frequency,
            }
            .exp()
        }
    }
}
