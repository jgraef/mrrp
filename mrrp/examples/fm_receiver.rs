use clap::Parser;
use color_eyre::eyre::Error;
use mrrp::{
    GetSampleRate,
    audio::play_audio,
    filter::{
        Decimate,
        biquad::BiquadDf2t,
    },
    io::{
        AsyncReadSamplesExt,
        LogSamplesInspector,
    },
    source::rtlsdr::RtlSdrSource,
};
use tokio::signal::ctrl_c;

use crate::demod::{
    DifferentiateAndAccessPhase,
    DifferentiateAndDivide,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    tracing::info!("AM receiver example");

    let args = Args::parse();

    // The desired audio sample rate.
    const AUDIO_SAMPLE_RATE: f32 = 44_100.0;

    // We use the default sampling rate of the RTL-SDR.
    // Note that we can't sample the RTL-SDR at our desired audio sampling rate.
    const RTLSDR_SAMPLE_RATE: f32 = 2_400_000.0; // 2.4 MHz

    const DECIMATION: usize = (RTLSDR_SAMPLE_RATE / AUDIO_SAMPLE_RATE) as usize;

    // Open the RTL-SDR and get a complex IQ stream
    let radio_source = RtlSdrSource::open(args.device, args.frequency, RTLSDR_SAMPLE_RATE).await?;

    let lowpass = BiquadDf2t::lowpass(radio_source, 150_000.0);
    //let demod = DifferentiateAndAccessPhase::new(RTLSDR_SAMPLE_RATE, 75_000.0);
    let demod = DifferentiateAndDivide::new(RTLSDR_SAMPLE_RATE, 75_000.0);
    let fm_demod = lowpass
        .scan_with(demod)
        .inspect_with(LogSamplesInspector::new(2_400_000));
    //let lowpass = AverageDecimate::new(fm_demod, DECIMATION);
    let lowpass = Decimate::new(BiquadDf2t::lowpass(fm_demod, 10_000.0), DECIMATION);

    println!("audio_sample_rate: {}", lowpass.sample_rate());

    // Now we can just play it!
    //
    // `play_audio` is just a helper function that turns our mrrp stream into a
    // rodio stream, creates an audio sink and plays the sound.
    play_audio(lowpass, args.volume)?;

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
    use std::f32::consts::{
        PI,
        TAU,
    };

    use mrrp::{
        filter::DelayLine,
        io::Scanner,
    };
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
