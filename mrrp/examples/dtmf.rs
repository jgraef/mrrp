use std::{
    convert::Infallible,
    fmt::Debug,
    path::{
        Path,
        PathBuf,
    },
};

use clap::Parser;
use color_eyre::eyre::{
    Error,
    eyre,
};
use futures_util::stream;
use mrrp::{
    audio::play_audio,
    filter::{
        GoertzelFilter,
        MovingAverage,
    },
    io::{
        AsyncReadSamplesExt,
        GetSampleRate,
        StreamLength,
        combinators::{
            Scanner,
            ScannerExt,
        },
    },
    modem::{
        dtmf::{
            DtmfEncoder,
            DtmfSymbol,
        },
        fm,
        sstv::{
            LEADER_TONE,
            PORCH_TONE,
            SYNC_TONE,
            VIS_HIGH_TONE,
            VIS_LOW_TONE,
        },
    },
    sink::file::write_stream_to_wav,
    source::file::WavSource,
};
use num_complex::Complex;
use plotters::{
    chart::{
        ChartBuilder,
        LabelAreaPosition,
    },
    prelude::{
        BitMapBackend,
        IntoDrawingArea,
    },
    series::LineSeries,
    style::{
        BLUE,
        GREEN,
        RED,
        RGBColor,
        WHITE,
    },
};

#[tokio::main]
async fn main() -> Result<(), color_eyre::eyre::Error> {
    let _ = dotenvy::dotenv();
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    tracing::info!("FM receiver example");

    let args = Args::parse();

    match args {
        Args::Encode {
            output,
            sample_rate,
            tone_duration,
            keys,
        } => {
            // parse symbols
            let symbols = keys
                .chars()
                .map(|c| DtmfSymbol::try_from(c))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|c| eyre!("Invalid DTMF symbol: {c}"))?;

            // convert to stream
            let symbols = stream::iter(
                symbols
                    .into_iter()
                    .map(|symbol| Ok::<_, Infallible>(symbol)),
            );

            // encode
            let encoded = DtmfEncoder::new(symbols, sample_rate, tone_duration);

            if let Some(output) = output {
                write_stream_to_wav(output, encoded).await?;
            }
            else {
                play_audio(encoded.map(|sample| sample.re), 0.5).await?;
            }
        }
    }

    Ok(())
}

#[derive(Debug, clap::Parser)]
enum Args {
    Encode {
        #[clap(short, long)]
        output: Option<PathBuf>,

        #[clap(short, long, default_value = "44100")]
        sample_rate: f32,

        #[clap(short, long, default_value = "0.5")]
        tone_duration: f32,

        keys: String,
    },
}

async fn plot(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<(), Error> {
    let source = WavSource::<_, Complex<f32>>::from_path(input)?;
    let sample_rate = source.sample_rate();
    let num_samples = source.len();

    println!("source sample rate: {sample_rate}");
    println!("source num samples: {num_samples}");

    let frequency_shift = 0.0;
    //let norm = |x: Complex<f32>| x.norm_sqr().log10() * 10.0;
    let norm = |x: Complex<f32>| x.norm();
    let mut leader_detect =
        GoertzelFilter::new(sample_rate, LEADER_TONE - frequency_shift, 100.0).map(norm);
    let mut sync_detect =
        GoertzelFilter::new(sample_rate, SYNC_TONE - frequency_shift, 100.0).map(norm);
    let mut porch_detect =
        GoertzelFilter::new(sample_rate, PORCH_TONE - frequency_shift, 100.0).map(norm);
    let mut vis_low =
        GoertzelFilter::new(sample_rate, VIS_LOW_TONE - frequency_shift, 100.0).map(norm);
    let mut vis_high =
        GoertzelFilter::new(sample_rate, VIS_HIGH_TONE - frequency_shift, 100.0).map(norm);
    let mut channel_low =
        GoertzelFilter::new(sample_rate, VIS_LOW_TONE - frequency_shift, 100.0).map(norm);
    let mut channel_high =
        GoertzelFilter::new(sample_rate, VIS_HIGH_TONE - frequency_shift, 100.0).map(norm);

    // this is somehow off by a factor of 2
    let fm_demod = fm::DifferentiateAndDivide::new(sample_rate, 1.0);
    //let fm_demod = fm::AccessPhaseAndDifferentiate::new(sample_rate, 1.0);
    //let fm_demod = fm::DifferentiateAndAccessPhase::new(sample_rate, 1.0);

    // smooth it
    let mut fm_demod = fm_demod.chain(MovingAverage::new(32));

    let mut mins = [0.0f32; 8];
    let mut maxs = [0.0f32; 8];
    let mut tone_detect = source
        //.convert::<Complex<f32>>()
        //.scan_with(lowpass.chain(fm::AccessPhaseAndDifferentiate::new(sample_rate, 1.0)));
        .map(|mut sample| {
            sample *= 0.01;
            let detect = [
                leader_detect.scan(sample),
                sync_detect.scan(sample),
                porch_detect.scan(sample),
                vis_low.scan(sample),
                vis_high.scan(sample),
                channel_low.scan(sample),
                channel_high.scan(sample),
                fm_demod.scan(sample),
            ];
            for (x, (min, max)) in detect.iter().zip(mins.iter_mut().zip(maxs.iter_mut())) {
                if *x < *min {
                    *min = *x;
                }
                if *x > *max {
                    *max = *x;
                }
            }
            detect
        });

    let mut tones = Vec::new();
    tone_detect.read_to_end(&mut tones).await?;
    let t_start = -0.05f32;
    let t_end = num_samples as f32 / sample_rate + 0.05;
    mins[7] = 1000.0;
    maxs[7] = 2000.0;

    let root = BitMapBackend::new(&output, (1080, 1080)).into_drawing_area();
    root.fill(&WHITE)?;
    let drawing_areas = root.split_evenly((8, 1));

    let draw_series = |label, color, index| {
        let mut chart = ChartBuilder::on(&drawing_areas[index])
            .margin(10)
            .caption(label, ("sans-serif", 20))
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 40)
            .build_cartesian_2d(t_start..t_end, mins[index]..maxs[index])?;
        chart
            .configure_mesh()
            .disable_mesh()
            .x_label_formatter(&|t| format!("{:.1} ms", t * 1000.0))
            .draw()?;

        chart.draw_series(LineSeries::new(
            tones
                .iter()
                .enumerate()
                .map(|(i, x)| (i as f32 / sample_rate, x[index])),
            &color,
        ))?;
        Ok::<(), color_eyre::eyre::Error>(())
    };

    draw_series("Leader (1900 Hz)", RED, 0)?;
    draw_series("Sync (1200 Hz)", BLUE, 1)?;
    draw_series("Porch (1500 Hz)", RGBColor(0, 0, 128), 2)?;
    draw_series("Vis Low (1300 Hz)", RGBColor(0, 128, 0), 3)?;
    draw_series("Vis High (1100 Hz)", GREEN, 4)?;
    draw_series("Channel Low (1500 Hz)", RGBColor(0, 128, 0), 5)?;
    draw_series("Channel High (2300 Hz)", GREEN, 6)?;
    draw_series("Frequency", RED, 7)?;

    root.present()?;

    Ok(())
}
