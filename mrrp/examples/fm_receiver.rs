use clap::Parser;
use color_eyre::eyre::Error;
use mrrp::{
    GetSampleRate,
    audio::play_audio,
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
    source::rtlsdr::RtlSdrSource,
};
use tokio::signal::ctrl_c;

use crate::demod::DifferentiateAndDivide;

// https://github.com/JulianKemmerer/PipelineC/wiki/Example:-FM-Radio-Demodulation

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    tracing::info!("FM receiver example");

    let args = Args::parse();

    // The desired audio sample rate.
    const AUDIO_SAMPLE_RATE: f32 = 44_100.0;

    // We use the default sampling rate of the RTL-SDR.
    // Note that we can't sample the RTL-SDR at our desired audio sampling rate.
    const RTLSDR_SAMPLE_RATE: f32 = 2_400_000.0; // 2.4 MHz

    // Open the RTL-SDR and get a complex IQ stream
    let baseband = RtlSdrSource::open(args.device, args.frequency, RTLSDR_SAMPLE_RATE).await?;

    // decimate down to 200 kHz by averaging.
    // the following FIR filter is better, but more expensive, so we decimate by a
    // factor of 6 first.
    //let baseband = AverageDecimate::new(baseband, 6);

    // use Remez to design a linear-phase lowpass filter
    let sample_rate = baseband.sample_rate();
    let filter_design = pm_remez(
        Lowpass::new(150000.0, 5000.0, 0.05, 0.005).normalize(sample_rate),
        11,
    )
    .unwrap();
    /*let filter_design = equiripple_fft(
        Lowpass::new(150000.0, 10000.0, 0.05, 0.05).normalize(sample_rate),
        17,
        512,
        |i, e| i >= 20 || e < 1e-6,
    )?;*/
    println!("filter design: {filter_design:#?}");

    let filtered_baseband = baseband.scan_in_place_with(filter_design.fir_filter());

    let demodulated =
        filtered_baseband.scan_with(DifferentiateAndDivide::new(sample_rate, 75_000.0));

    let sample_rate = demodulated.sample_rate();
    let filtered = demodulated
        .scan_in_place_with(biquad::lowpass(sample_rate, 0.5 * AUDIO_SAMPLE_RATE))
        .decimate_to(AUDIO_SAMPLE_RATE);

    println!("audio_sample_rate: {}", filtered.sample_rate());

    // Now we can just play it!
    //
    // `play_audio` is just a helper function that turns our mrrp stream into a
    // rodio stream, creates an audio sink and plays the sound.
    play_audio(filtered, args.volume)?;

    let _ = ctrl_c().await;
    println!("Ctrl-C pressed. Aborting.");

    Ok(())
}

#[derive(Debug, clap::Parser)]
struct Args {
    frequency: f32,

    #[clap(short, long, default_value = "0")]
    device: u32,

    #[clap(short, long, default_value = "0.2")]
    volume: f32,
}

mod demod {
    #![allow(dead_code)]

    use std::f32::consts::TAU;

    use mrrp::io::Scanner;
    use num_complex::Complex;
    use num_traits::Zero;

    /// https://wirelesspi.com/frequency-modulation-fm-and-demodulation-using-dsp-techniques/
    #[derive(Clone, Copy, Debug)]
    pub struct DifferentiateAndAccessPhase {
        delayed: Complex<f32>,
        norm_factor: f32,
    }

    impl DifferentiateAndAccessPhase {
        pub fn new(sample_rate: f32, frequency_deviation: f32) -> Self {
            Self {
                delayed: Complex::ZERO,
                norm_factor: sample_rate / (TAU * frequency_deviation),
            }
        }
    }

    impl Scanner<Complex<f32>> for DifferentiateAndAccessPhase {
        type Output = f32;

        fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
            let phase_difference = (self.delayed.conj() * sample).arg();
            self.delayed = sample;
            phase_difference * self.norm_factor
        }
    }

    /// https://wirelesspi.com/frequency-modulation-fm-and-demodulation-using-dsp-techniques/
    /// buggy
    #[derive(Clone, Copy, Debug)]
    pub struct AccessPhaseAndDifferentiate {
        delayed: f32,
        norm_factor: f32,
    }

    impl AccessPhaseAndDifferentiate {
        pub fn new(sample_rate: f32, frequency_deviation: f32) -> Self {
            Self {
                delayed: 0.0,
                norm_factor: sample_rate / (TAU * frequency_deviation),
            }
        }
    }

    impl Scanner<Complex<f32>> for AccessPhaseAndDifferentiate {
        type Output = f32;

        fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
            let phase = sample.arg();
            let phase_difference = phase - self.delayed;
            self.delayed = phase;
            phase_difference * self.norm_factor
        }
    }

    /// [Slide 12](https://cci.usc.edu/wp-content/uploads/2017/09/CLASS-6-FM-modulation.pdf)
    #[derive(Clone, Copy, Debug)]
    pub struct DifferentiateAndDivide {
        delay1: Complex<f32>,
        delay2: Complex<f32>,
        norm_factor: f32,
    }

    impl DifferentiateAndDivide {
        pub fn new(sample_rate: f32, frequency_deviation: f32) -> Self {
            Self {
                delay1: Complex::zero(),
                delay2: Complex::zero(),
                norm_factor: sample_rate / (TAU * frequency_deviation),
            }
        }
    }

    impl Scanner<Complex<f32>> for DifferentiateAndDivide {
        type Output = f32;

        fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
            let output = if self.delay1.is_zero() {
                // don't divide by 0
                0.0
            }
            else {
                let a = (sample.re - self.delay2.re) * self.delay1.im;
                let b = (sample.im - self.delay2.im) * self.delay1.re;
                (b - a) / self.delay1.norm_sqr() * self.norm_factor
            };

            self.delay2 = self.delay1;
            self.delay1 = sample;

            output
        }
    }
}
