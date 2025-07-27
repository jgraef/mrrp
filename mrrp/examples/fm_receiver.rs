use clap::Parser;
use color_eyre::eyre::Error;
use mrrp::{
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
    io::{
        AsyncReadSamplesExt,
        GetSampleRate,
    },
    modem::fm::FmDemodulator,
    source::rtlsdr::RtlSdrSource,
};
use tokio::signal::ctrl_c;

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

    let demodulated = filtered_baseband.scan_with(FmDemodulator::new(sample_rate, 75_000.0));

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
