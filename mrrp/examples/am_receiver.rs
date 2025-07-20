use std::{
    sync::{
        Arc,
        atomic::{
            AtomicUsize,
            Ordering,
        },
    },
    time::Duration,
};

use clap::Parser;
use color_eyre::eyre::Error;
use mrrp::{
    audio::play_audio,
    filter::{
        AverageDecimate,
        biquad::BiquadDf2t,
    },
    io::AsyncReadSamplesExt,
    source::rtlsdr::RtlSdrSource,
};
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    tracing::info!("AM receiver example");

    let args = Args::parse();

    // AM radio usually has a bandwidth of 10 kHz, so our sample rate should be
    // twice that.
    const AUDIO_SAMPLE_RATE: f32 = 20_000.0;

    // We can't sample the RTL-SDR at the desired audio sampling rate. Refer to
    // `rtlsdr_async::RtlSdr::set_sample_rate` for valid sample rates.
    // So we'll need to sample higher and then reduce the sample rate later. This is
    // best done if we sample at a multiple of the desired sampling rate.
    const DECIMATION: usize = 64;
    const RTLSDR_SAMPLE_RATE: f32 = AUDIO_SAMPLE_RATE * DECIMATION as f32; // 1.28 MHz

    // Open the RTL-SDR and get a complex IQ stream
    let radio_source = RtlSdrSource::open(args.device, args.frequency, RTLSDR_SAMPLE_RATE).await?;

    // This will just keep track of how many samples we have processed
    let num_samples = Arc::new(AtomicUsize::new(0));
    let radio_source = radio_source.inspect({
        let num_samples = num_samples.clone();
        move |samples| {
            num_samples.fetch_add(samples.len(), Ordering::Relaxed);
        }
    });

    // Next we'll downsample the signal, but to avoid aliasing we first need
    // to pass it through a lowpass filter that only allows the desired frequencies
    // through.
    //
    // To downsample we'll just discard 63 out of 64 samples to go from a sample
    // rate of 1.28 MHz to 20 kHz. Since computing the low-pass filter is
    // somewhat expensive and we would be throwing away most of it anyway, this
    // step is combined in one filter.
    //
    // The low-pass filter used is just taking the average. While it's a very simple
    // filter, it's frequency response is actually not that good.
    //let lowpass_filtered = AverageDecimate::new(radio_source, DECIMATION);
    let lowpass_filtered = BiquadDf2t::lowpass(radio_source, 5000.0);

    // Next we'll map the complex signal to a real amplitude by taking the norm of
    // the samples.
    let audio = lowpass_filtered.map(|complex_sample| complex_sample.norm());

    // Now we can just play it!
    //
    // `play_audio` is just a helper function that turns our mrrp stream into a
    // rodio stream, creates an audio sink and plays the sound.
    play_audio(audio, args.volume)?;

    // Every 2 seconds we'll print the number of samples we have processed so far.
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        tokio::select! {
            // Abort on Ctrl-C
            _ = ctrl_c() => {
                println!("Ctrl-C pressed. Aborting.");
                break
            },
            // Print number of samples
            _ = interval.tick() => {
                println!("Processed {} samples", num_samples.load(Ordering::Relaxed));
            }
        }
    }

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
