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

use anyhow::Error;
use clap::Parser;
use futures_util::pin_mut;
use mrrp::{
    audio::play_audio,
    filter::resampling::AverageDecimate,
    rtl_sdr,
    signal::{
        AsyncReadSamplesExt,
        GetSampleRate,
    },
};
use num_complex::Complex;
use tokio::signal::ctrl_c;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt::init();
    tracing::info!("AM receiver example");

    let args = Args::parse();

    // The desired audio sample rate.
    const AUDIO_SAMPLE_RATE: f32 = 44_100.0;

    // We use the default sampling rate of the RTL-SDR.
    // Note that we can't sample the RTL-SDR at our desired audio sampling rate.
    const RTLSDR_SAMPLE_RATE: f32 = 2_400_000.0; // 2.4 MHz

    // We will need to downsample from 2.4 MHz to 44.1 kHz.
    // We choose an integer downsampling factor for this. Note that we'll still
    // sample the RTL-SDR at 2.4 MHz, but after downsampling with this rounded
    // factor our resulting audio sampling rate might be a bit off. It doesn't
    // matter though as mrrp keeps track of the sampling rate through the whole
    // pipeline for us.
    const DECIMATION: usize = (RTLSDR_SAMPLE_RATE / AUDIO_SAMPLE_RATE) as usize;

    // Open the RTL-SDR and get a complex IQ stream
    let mut rtl_sdr = rtl_sdr::open_any(Default::default()).await?;
    rtl_sdr.set_sample_rate(RTLSDR_SAMPLE_RATE).await?;
    rtl_sdr.set_center_frequency(args.frequency).await?;
    let radio_source = rtl_sdr.reader(Default::default()).await?;

    // This will just keep track of how many samples we have processed
    let num_samples = Arc::new(AtomicUsize::new(0));
    let num_samples2 = num_samples.clone();
    let radio_source = radio_source.inspect(move |samples| {
        num_samples2.fetch_add(samples.len(), Ordering::Relaxed);
    });

    // convert u8 to f32
    let converted = radio_source.convert::<Complex<f32>>();

    // Downsample the signal, but to avoid aliasing we first need to pass it through
    // a lowpass filter that only allows the desired frequencies through.
    //
    // To downsample we just discard all but one sample every `DECIMATION` samples.
    //
    // Since computing the low-pass filter is somewhat expensive and we would be
    // throwing away most of it anyway, this step is combined in one filter.
    //
    // The low-pass filter used is just taking the average. While it's a very simple
    // filter, it's frequency response is actually not that good.
    let lowpass_filtered = AverageDecimate::new(converted, DECIMATION);

    // Map the complex signal to a real amplitude by taking the norm of the samples.
    let audio = lowpass_filtered.map(|complex_sample| complex_sample.norm());
    println!("audio_sample_rate: {}", audio.sample_rate());

    // Now we can just play it!
    //
    // `play_audio` is just a helper function that turns our mrrp stream into a
    // rodio stream, creates an audio sink and plays the sound.
    //
    // The future it returns will resolve once the stream stops playing and will
    // return any errors produced by our stream.
    let playback_future = play_audio(audio, args.volume);
    pin_mut!(playback_future);

    // Every 2 seconds we'll print the number of samples we have processed so far.
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        tokio::select! {
            result = &mut playback_future => {
                result?;
                break;
            }
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

    #[clap(short, long, default_value = "0.2")]
    volume: f32,
}
