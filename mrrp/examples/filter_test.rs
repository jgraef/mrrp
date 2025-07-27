use std::path::PathBuf;

use clap::Parser;
use color_eyre::eyre::{
    Error,
    bail,
};
use mrrp::{
    filter::{
        biquad,
        design::{
            Estimate,
            Lowpass,
            Normalize,
            Normalized,
            argmin,
            equiripple_fft,
            pm_remez,
        },
        fir::FirFilter,
    },
    io::{
        AsyncReadSamplesExt,
        combinators::Scanner,
    },
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

    println!("cutoff frequency: {}", args.cutoff_frequency());
    println!("transition bandwidth: {}", args.transition_bandwidth());

    let filter: Box<dyn Scanner<Complex<f32>, Output = Complex<f32>> + Send> =
        if let Some(filter) = &args.filter {
            match filter.as_str() {
                "biquad" => Box::new(biquad::lowpass(args.sample_rate, args.cutoff_frequency())),
                "fir-halfband" => {
                    let coefficients = fir_half_band_lowpass();
                    Box::new(FirFilter::new(coefficients))
                }
                "fir-equiripple-fft" => {
                    let filter_design = equiripple_fft::equiripple_fft(
                        args.filter_specification(),
                        Estimate,
                        None,
                        |i, e| e < 1e-6 && i >= 20,
                    )
                    .unwrap();
                    println!("filter design: {filter_design:#?}");
                    let coefficients = filter_design.coefficients;
                    Box::new(FirFilter::new(coefficients))
                }
                "fir-fft-particleswarm" => {
                    let coefficients =
                        argmin::particle_swarm_fft(args.filter_specification(), Estimate, None)
                            .unwrap();
                    println!("filter design: {coefficients:#?}");
                    Box::new(FirFilter::new(coefficients))
                }
                "fir-pmremez" => {
                    let design = pm_remez::pm_remez(args.filter_specification(), 11)?;
                    println!("filter design: {design:#?}");
                    Box::new(FirFilter::new(design.impulse_response))
                }
                _ => bail!("Unknown filter type: {filter}"),
            }
        }
        else {
            Box::new(())
        };

    let output = noise.scan_in_place_with(filter);

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

    #[clap(short, long)]
    transition_bandwidth: Option<f32>,

    #[clap(short = 'o', long)]
    file_output: Option<PathBuf>,

    #[clap(long)]
    tcp_output: Option<String>,

    #[clap(short = 'F', long)]
    filter: Option<String>,
}

impl Args {
    fn cutoff_frequency(&self) -> f32 {
        self.cutoff_frequency
            .unwrap_or_else(|| self.sample_rate / 4.0)
    }

    fn transition_bandwidth(&self) -> f32 {
        self.transition_bandwidth
            .unwrap_or_else(|| self.cutoff_frequency() * 0.4)
    }

    fn filter_specification(&self) -> Normalized<Lowpass> {
        Lowpass::new(
            self.cutoff_frequency(),
            self.transition_bandwidth(),
            0.05,
            0.05,
        )
        .normalize(self.sample_rate)
    }
}

fn fir_half_band_lowpass() -> Vec<f32> {
    // taken from https://yoksis.bilkent.edu.tr/pdf/files/10.1109-79.581378.pdf
    let mut coefficients = vec![
        0.5261e-1,
        -5.7907e-7,
        -0.9116e-1,
        -2.6443e-16,
        0.3130e0,
        0.5e0,
    ];
    coefficients.resize(11, 0.0);
    for i in 0..5 {
        coefficients[6 + i] = coefficients[4 - i];
    }
    coefficients
}
