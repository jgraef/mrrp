use std::{
    collections::VecDeque,
    path::{
        Path,
        PathBuf,
    },
};

use clap::Parser;
use color_eyre::eyre::Error;
use image::{
    ImageReader,
    RgbImage,
};
use mrrp::{
    filter::{
        GoertzelFilter,
        resampling::AverageDecimate,
    },
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        EofError,
        GetSampleRate,
        StreamLength,
        combinators::{
            ScanWith,
            Scanner,
            ScannerExt,
        },
    },
    modem::{
        fm,
        sstv::{
            HEADER_LEADER_TIME,
            HEADER_LEADER_TONE,
            HEADER_VIS_HIGH_TONE,
            HEADER_VIS_LOW_TONE,
            ModeSpecification,
            SstvEncoder,
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
            image,
            output,
            sample_rate,
        } => encode_image(&image, &output, sample_rate).await?,
        Args::Decode { input, output } => decode_image(&input, &output).await?,
    }

    Ok(())
}

#[derive(Debug, clap::Parser)]
enum Args {
    Encode {
        image: PathBuf,
        output: PathBuf,
        #[clap(short, long, default_value = "32000")]
        sample_rate: f32,
    },
    Decode {
        input: PathBuf,
        output: PathBuf,
    },
}

async fn encode_image(
    image: impl AsRef<Path>,
    output: impl AsRef<Path>,
    sample_rate: f32,
) -> Result<(), Error> {
    let image = ImageReader::open(image)?.decode()?.into_rgb8();
    let stream = SstvEncoder::new(image, ModeSpecification::M2, sample_rate);
    write_stream_to_wav(output, stream).await?;
    Ok(())
}

async fn decode_image(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<(), Error> {
    //let source = WavSource::<_, f32>::from_path("tmp/Martin_1.wav")?;
    let source = WavSource::<_, Complex<f32>>::from_path(input)?;
    let sample_rate = source.sample_rate();
    let num_samples = source.len();

    println!("source sample rate: {sample_rate}");
    println!("source num samples: {num_samples}");

    //let hilbert_filter = HilbertFilter::new(0.1, 11);

    //let output = source.scan_with(hilbert_filter).interpolate_to(2400000.0);
    //write_stream_to_wav("sstv_iq.wav", output).await?;

    //let fm_demod = fm::DifferentiateAndDivide::new(sample_rate, 3000.0);

    //let mut downsampled = AverageDecimate::<_, f32>::new(fm, 22);
    // //.buffered(0x4000);

    /*let mut decoder = SstvDecoder::new(source.convert::<Complex<f32>>());
    while let Some(image) = decoder.decode().await? {
        todo!();
    }*/

    /*let lowpass = pm_remez(
        Lowpass::new(1000.0, 10.0, 1.0, 1.0).normalize(sample_rate),
        41,
    )?
    .fir_filter();*/

    //let lowpass2 =
    // pm_remez(Lowpass::new(10.0, 1.0, 1.0, 1.0).normalize(sample_rate),
    // 41)?.fir_filter();

    // vis high: 1100
    // header break, vis stop, line sync: 1200
    // vis low: 1300
    // line break: 1500
    // header leader: 1900

    let frequency_shift = 0.0;
    //let norm = |x: Complex<f32>| x.norm_sqr().log10() * 10.0;
    let norm = |x: Complex<f32>| x.norm();
    let mut leader_detect =
        GoertzelFilter::new(sample_rate, HEADER_LEADER_TONE - frequency_shift, 100.0).map(norm);
    let mut sync_detect =
        GoertzelFilter::new(sample_rate, LINE_SYNC_TONE - frequency_shift, 100.0).map(norm);
    let mut vis_low =
        GoertzelFilter::new(sample_rate, HEADER_VIS_LOW_TONE - frequency_shift, 100.0).map(norm);
    let mut vis_high =
        GoertzelFilter::new(sample_rate, HEADER_VIS_HIGH_TONE - frequency_shift, 100.0).map(norm);
    let mut fm_demod = fm::DifferentiateAndAccessPhase::new(sample_rate, 2000.0);

    let mut max_detect = 0.0f32;
    let mut tone_detect = source
        //.convert::<Complex<f32>>()
        //.scan_with(lowpass.chain(fm::AccessPhaseAndDifferentiate::new(sample_rate, 1.0)));
        .map(|mut sample| {
            sample *= 0.01;
            let detect = [
                leader_detect.scan(sample),
                sync_detect.scan(sample),
                vis_low.scan(sample),
                vis_high.scan(sample),
                fm_demod.scan(sample),
            ];
            for x in &detect {
                if *x > max_detect {
                    max_detect = *x;
                }
            }
            detect
        });

    let mut tones = Vec::new();
    tone_detect.read_to_end(&mut tones).await?;
    let t_start = -0.05f32;
    let t_end = num_samples as f32 / sample_rate + 0.05;

    let root = BitMapBackend::new("sstv_freq.png", (1080, 1080)).into_drawing_area();
    root.fill(&WHITE)?;
    let drawing_areas = root.split_evenly((5, 1));

    let draw_series = |label, color, index| {
        let mut chart = ChartBuilder::on(&drawing_areas[index])
            .margin(10)
            .caption(label, ("sans-serif", 20))
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 40)
            .build_cartesian_2d(t_start..t_end, -0.2f32..max_detect * 1.2)?;
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

    draw_series("Frequency", RED, 4)?;
    draw_series("Leader", RED, 0)?;
    draw_series("Sync", BLUE, 1)?;
    draw_series("Vis Low", RGBColor(0, 128, 0), 2)?;
    draw_series("Vis High", RGBColor(0, 255, 0), 3)?;

    root.present()?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
enum DecodeError<R> {
    Stream(#[source] R),
    Eof,
    Invalid,
}

// http://lionel.cordesses.free.fr/gpages/sstv.html
// https://web.archive.org/web/20120505141047/http://www.cs.helsinki.fi/u/okraisan/slowrx/
// header: http://www.barberdsp.com/downloads/Dayton%20Paper.pdf
// very good: https://web.archive.org/web/20120313215600/http://lionel.cordesses.free.fr/gpages/Cordesses.pdf
#[derive(Debug)]
struct SstvDecoder<R> {
    tones: AverageDecimate<ScanWith<R, Complex<f32>, fm::AccessPhaseAndDifferentiate>, f32>,
    //tones: ScanWith<R, Complex<f32>, fm::AccessPhaseAndDifferentiate>,
    sample_time: f32,
    num_samples_consumed: usize,
    peeked: VecDeque<f32>,
}

impl<R> SstvDecoder<R>
where
    R: GetSampleRate + AsyncReadSamples<Complex<f32>> + Unpin,
    R::Error: Send + Sync + 'static,
{
    pub fn new(input: R) -> Self {
        let sample_rate = input.sample_rate();

        // passing 1 as frequency deviation only affects the normalization factor, thus
        // giving the frequency in Hz
        let fm = fm::AccessPhaseAndDifferentiate::new(sample_rate, 1.0);

        let tones = AverageDecimate::new(input.scan_with(fm), 6);
        //let tones = input.scan_with(fm);

        Self {
            tones,
            sample_time: 1.0 / sample_rate,
            num_samples_consumed: 0,
            peeked: VecDeque::new(),
        }
    }

    async fn next_tone(&mut self) -> Result<f32, DecodeError<R::Error>> {
        let tone = if let Some(tone) = self.peeked.pop_front() {
            tone
        }
        else {
            let tone = self.tones.read_sample().await.map_err(|error| {
                match error {
                    EofError::Eof {
                        num_samples_read: _,
                    } => DecodeError::Eof,
                    EofError::Other(error) => DecodeError::Stream(error),
                }
            })?;
            //tone + 950.0

            tone
        };
        self.num_samples_consumed += 1;
        Ok(tone)
    }

    fn put_back_tone(&mut self, tone: f32) {
        self.peeked.push_back(tone);
        self.num_samples_consumed -= 1;
    }

    async fn wait_for_tone(&mut self, expected: f32) -> Result<(), DecodeError<R::Error>> {
        loop {
            let tone = self.next_tone().await?;
            if is_tone(tone, expected) {
                println!("got tone: {tone} at {}", self.num_samples_consumed);
                self.put_back_tone(tone);
                return Ok(());
            }
        }
    }

    async fn consume_tone(
        &mut self,
        expected_tone: f32,
        expected_duration: f32,
    ) -> Result<f32, DecodeError<R::Error>> {
        let mut t = 0.0;
        let duration_tolerance = 0.1;

        //println!("starting to consume tone: {expected_tone}");

        loop {
            let tone = self.next_tone().await?;

            if !is_tone(tone, expected_tone) {
                //self.put_back_tone(tone);
                //println!("consumed tone: {t}");
                break;
            }
            else {
                println!("{}: {tone} Hz for {t}", self.num_samples_consumed);
            }
            t += self.sample_time;
        }

        if t > expected_duration - duration_tolerance && t < expected_duration + duration_tolerance
        {
            println!("tone {expected_tone} Hz for {t} s");
            Ok(t)
        }
        else {
            Err(DecodeError::Invalid)
        }
    }

    async fn decode_inner(&mut self) -> Result<RgbImage, DecodeError<R::Error>> {
        //self.wait_for_tone(HEADER_LEADER_TONE).await?;

        self.consume_tone(HEADER_LEADER_TONE, HEADER_LEADER_TIME)
            .await?;
        //dbg!(leader1_tone_duration);

        //let break_tone_duration = self.consume_tone(HEADER_BREAK_TONE).await?;
        //dbg!(break_tone_duration);

        //let leader2_tone_duration = self.consume_tone(HEADER_LEADER_TONE).await?;
        //dbg!(leader2_tone_duration);*/
        for i in 0..100 {
            println!("{}", self.next_tone().await?);
        }

        todo!();
    }

    pub async fn decode(&mut self) -> Result<Option<RgbImage>, Error> {
        loop {
            match self.decode_inner().await {
                Ok(image) => return Ok(Some(image)),
                Err(DecodeError::Eof) => return Ok(None),
                Err(DecodeError::Stream(error)) => return Err(error.into()),
                Err(DecodeError::Invalid) => {}
            }
        }
    }

    fn as_secs(&self, samples: usize) -> f32 {
        samples as f32 * self.sample_time
    }

    fn as_samples(&self, secs: f32) -> usize {
        (secs / self.sample_time) as usize
    }
}

#[inline]
fn is_tone(frequency: f32, tone: f32) -> bool {
    frequency > tone - TONE_TOLERANCE && frequency < tone + TONE_TOLERANCE
}

const TONE_TOLERANCE: f32 = 50.0;

const LINE_SYNC_TONE: f32 = 1200.0;
const LINE_SYNC_TIME: f32 = 0.004862;
const LINE_BREAK_TONE: f32 = 1500.0;
const LINE_BREAK_TIME: f32 = 0.000572;
const LINE_LUM_LOW_TONE: f32 = 1500.0;
const LINE_LUM_HIGH_TONE: f32 = 2300.0;
const LINE_LUM_TIME: f32 = 0.146432;
