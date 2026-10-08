use std::path::PathBuf;

use anyhow::Error;
use clap::Parser;
use mrrp_audio::WavSource;
use mrrp_sstv::modes::ModeSpecification;
use mrrp_util::signal::AsyncReadSamplesExt;

use crate::Context;

pub async fn run(context: Context<Args>) -> Result<(), Error> {
    let mut source = WavSource::<_, i16>::from_path(&context.args.path)?.convert::<f32>();
    tracing::info!(spec = ?source.inner().spec(), "input");

    let vis_code = vis::decode(&mut source).await?;
    assert_eq!(vis_code, ModeSpecification::R36.vis_code);

    let image = attempt1::decode(source).await?;

    tracing::info!(
        width = image.width(),
        height = image.height(),
        ?vis_code,
        "output"
    );
    image.save(context.args.output)?;

    Ok(())
}

#[derive(Debug, Parser)]
pub struct Args {
    #[clap(short, long)]
    output: PathBuf,

    path: PathBuf,
}

mod vis {
    use anyhow::{
        Error,
        anyhow,
    };
    use mrrp_core::signal::{
        AsyncReadSamples,
        GetSampleRate,
    };
    use mrrp_sstv::{
        LEADER_TIME,
        LEADER_TONE,
        SYNC_TONE,
        VIS_BIT_TIME,
        VIS_HIGH_TONE,
        VIS_LOW_TONE,
        modes::VisCode,
    };
    use mrrp_util::signal::{
        AsyncReadSamplesExt,
        Scanner,
    };

    use crate::commands::test::sstv::{
        Edge,
        attempt1::{
            FrequencyCounter,
            ToneDetect,
        },
    };

    /// Detects calibration header with VIS code
    ///
    /// Note that this expects the demouldated frequencies as input. Depending
    /// on whether the input signal is audio, or IQ, you have different options
    /// on how to detect the frequency.
    #[derive(Clone, Debug)]
    pub struct VisDetect {
        /// sample rate
        ///
        /// we only need this in the constructor to calculate tone lengths, but
        /// we need this atm for debugging
        sample_rate: f32,

        // tone detectors
        leader_detect: ToneDetect,
        vis_low_detect: ToneDetect,
        vis_high_detect: ToneDetect,
        vis_start_stop_detect: ToneDetect,

        // tone lengths (in samples)
        min_leader_length: usize,
        vis_bit_length: usize,

        // state
        sample_num: usize,
        state: State,
    }

    #[derive(Clone, Copy, Debug, Default)]
    struct State {
        leader_count: usize,
        leader_start_time: Option<usize>,
        vis_bit_start_time: Option<usize>,
        vis_code_buffer: u8,
        vis_bit_count: usize,
        vis_bit_votes: i32,
    }

    impl VisDetect {
        pub fn new(sample_rate: f32) -> Self {
            let leader_detect = ToneDetect::new(LEADER_TONE, 50.0, 75.0);
            let vis_low_detect = ToneDetect::new(VIS_LOW_TONE, 50.0, 75.0);
            let vis_high_detect = ToneDetect::new(VIS_HIGH_TONE, 50.0, 75.0);
            let vis_start_stop_detect = ToneDetect::new(SYNC_TONE, 50.0, 75.0);

            let min_leader_length = (LEADER_TIME * sample_rate * 0.75) as usize;
            let vis_bit_length = (VIS_BIT_TIME * sample_rate) as usize;

            Self {
                leader_detect,
                vis_low_detect,
                vis_high_detect,
                vis_start_stop_detect,
                sample_rate,
                min_leader_length,
                vis_bit_length,
                sample_num: 0,
                state: Default::default(),
            }
        }

        #[inline]
        pub fn reset(&mut self) {
            self.state = Default::default();
        }
    }

    impl Scanner<f32> for VisDetect {
        type Output = Option<VisCode>;

        fn scan(&mut self, frequency: f32) -> Self::Output {
            if self.state.vis_bit_count == 8 {
                // reset
                self.reset();
            }

            // this is only used for debugging
            let t = self.sample_num as f32 / self.sample_rate;
            //println!("t={:.3}ms, f={frequency}", t * 1000.0);

            let (_leader_state, leader_edge) = self.leader_detect.scan(frequency);
            match leader_edge {
                Some(Edge::Rising) => {
                    // start of a leader pulse
                    self.state.leader_start_time = Some(self.sample_num)
                }
                Some(Edge::Falling) => {
                    // end of a leader pulse. but we want to ignore leader
                    // pulses that have wrong timings.

                    let leader_start_time = self
                        .state
                        .leader_start_time
                        .take()
                        .expect("leader falling edge without prior raising edge");

                    let leader_time = self.sample_num - leader_start_time;
                    if leader_time > self.min_leader_length {
                        self.state.leader_count += 1;
                        println!(
                            "LEADER: t={:.3}ms, n={}, dt={:.3}ms",
                            t * 1000.0,
                            self.state.leader_count,
                            leader_time as f32 / self.sample_rate * 1000.0
                        );
                    }
                }
                _ => {}
            }

            // before the VIS code there will be 2 leader pulses
            if self.state.leader_count >= 2 {
                // the vis code is surrounded by 2 pulses, but the bits are not
                // separated, so we have to rely on timing. at start
                // `vis_bit_start_time` will be `None`, so look at the else case
                // of the following if.

                let (_start_stop_state, start_stop_edge) =
                    self.vis_start_stop_detect.scan(frequency);

                if let Some(bit_start_time) = &mut self.state.vis_bit_start_time {
                    // a bit has already started

                    if self.sample_num - *bit_start_time >= self.vis_bit_length {
                        // the current bit is complete

                        *bit_start_time = self.sample_num;

                        // vis code is least significant bit first. so we shift
                        // right and write into the msb.
                        self.state.vis_code_buffer >>= 1;

                        // high tones vote positive, low tones vote negative.
                        if self.state.vis_bit_votes > 0 {
                            // high bit
                            self.state.vis_code_buffer |= 0x80;
                        }
                        // note: nothing to do for the low bit

                        println!(
                            "VIS CODE so far: 0x{:02x}, t={:.3}ms, votes={}",
                            self.state.vis_code_buffer,
                            t * 1000.0,
                            self.state.vis_bit_votes
                        );

                        // reset votes
                        self.state.vis_bit_votes = 0;
                        // increment bit count
                        self.state.vis_bit_count += 1;
                    }

                    if self.state.vis_bit_count == 8 {
                        // we accumulated 8 bits in the buffer.

                        // todo: this contains the parity bit - although the
                        // type invariant is that the msb is 0.
                        let vis_code = VisCode::new_unchecked(self.state.vis_code_buffer);
                        return Some(vis_code);
                    }

                    if let Some(Edge::Rising) = start_stop_edge {
                        // normally the vis code is over here, but we get lots
                        // of spurious detections

                        println!("VIS STOP: t={:.3}ms", t * 1000.0);
                        //break;
                    }

                    // detect low or high bit tone
                    let (low_state, low_edge) = self.vis_low_detect.scan(frequency);
                    let (high_state, high_edge) = self.vis_high_detect.scan(frequency);

                    // this just votes negative and or positive depending on
                    // what tone was detected
                    if low_state {
                        self.state.vis_bit_votes -= 1
                    };
                    if high_state {
                        self.state.vis_bit_votes += 1
                    };

                    // only for debugging
                    if let Some(Edge::Falling) = low_edge {
                        println!("LOW: t={:.3}ms", t * 1000.0);
                    }
                    else if let Some(Edge::Falling) = high_edge {
                        println!("HIGH: t={:.3}ms", t * 1000.0);
                    }
                }
                else if let Some(Edge::Falling) = start_stop_edge {
                    // no bit has started yet, but we detected the falling edge
                    // of the start pulse. so the first bit is starting now.

                    println!("VIS START: t={:.3}ms", t * 1000.0);
                    self.state.vis_bit_start_time = Some(self.sample_num);
                }
            }

            self.sample_num += 1;

            None
        }
    }

    pub async fn decode<S>(mut source: S) -> Result<VisCode, Error>
    where
        S: AsyncReadSamples<Sample = f32> + GetSampleRate + Unpin,
        Error: From<S::Error>,
    {
        let sample_rate = source.sample_rate();

        let mut frequency_counter = FrequencyCounter::new(50.0, sample_rate);
        let mut vis_detect = VisDetect::new(sample_rate);

        while let Some(sample) = source.read_sample_or_eof().await? {
            let frequency = frequency_counter.scan(sample);
            if let Some(vis_code) = vis_detect.scan(frequency) {
                return Ok(vis_code);
            }
        }

        Err(anyhow!("No VIS header detected"))
    }
}

mod attempt1 {
    use anyhow::Error;
    use image::{
        DynamicImage,
        RgbImage,
        imageops::FilterType,
    };
    use mrrp_core::signal::{
        AsyncReadSamples,
        GetSampleRate,
    };
    use mrrp_filter::MovingSum;
    use mrrp_sstv::{
        CHANNEL_HIGH_TONE,
        CHANNEL_LOW_TONE,
        SYNC_TONE,
        color::{
            Rgb,
            YCbCr,
        },
        modes::ModeSpecification,
    };
    use mrrp_util::signal::{
        AsyncReadSamplesExt,
        Scanner,
    };

    use crate::commands::test::sstv::Edge;

    pub async fn decode<S>(mut source: S) -> Result<DynamicImage, Error>
    where
        S: AsyncReadSamples<Sample = f32> + GetSampleRate + Unpin,
        Error: From<S::Error>,
    {
        let sample_rate = source.sample_rate();

        let mut coarse_frequency = FrequencyCounter::new(100.0, sample_rate);
        let r = (CHANNEL_HIGH_TONE - CHANNEL_LOW_TONE) / 4.0;
        let mut fine_frequency = FrequencyCounter::new(r, sample_rate);
        let mut sync_detect = ToneDetect::new(SYNC_TONE, 100.0, 110.0);
        //let mut porch_detect = ToneDetect::new(PORCH_TONE, 100.0, 120.0);

        let mut lines = vec![];
        let mut line = vec![];
        let mut previous_sync_time = 0;
        let mut num_syncs = 0;

        let mut i = 0;
        while let Some(sample) = source.read_sample_or_eof().await? {
            let coarse_frequency = coarse_frequency.scan(sample);

            let t = i as f32 / sample_rate;
            let time_since_sync = (i - previous_sync_time) as f32 / sample_rate;

            let (sync_state, sync_edge) = sync_detect.scan(coarse_frequency);
            //let (porch_state, porch_edge) =
            // porch_detect.scan(fine_frequency);

            /*match porch_edge {
                Some(Edge::Rising) => println!("porch start: t={t:.3}s"),
                Some(Edge::Falling) => println!("porch start: t={t:.3}s"),
                _ => {}
            }*/

            if !sync_state {
                let fine_frequency = fine_frequency.scan(sample);
                line.push(fine_frequency);
            }

            if let Some(Edge::Rising) = sync_edge {
                num_syncs += 1;
                println!(
                    "t={t:.3}s SYNC! f={coarse_frequency} Hz, n = {num_syncs}, dt={:.3}ms, line={}",
                    time_since_sync * 1000.0,
                    line.len()
                );

                previous_sync_time = i;

                let line = std::mem::take(&mut line);
                if line.len() > 6000 && line.len() < 8000 {
                    lines.push(line);
                }
            }

            i += 1;
        }

        let sync_porch_len = (0.003 * sample_rate).round() as usize;
        let y_len = (0.088 * sample_rate).round() as usize;
        let sep_len = (0.006 * sample_rate).round() as usize;
        let uv_len = (0.044 * sample_rate).round() as usize;

        //let width = lines.iter().map(|line| line.len()).max().unwrap() as u32
        // * 2 / 3 + 2;
        let width = y_len as u32;
        let height = lines.len() as u32;
        tracing::debug!(?width, ?height);

        let mut image = RgbImage::new(width, height);
        // Y from two lines, then U and V from even and odd lines respectively
        let mut yuv_buffer: Vec<[f32; 4]> = vec![Default::default(); width as usize];

        for (y, line) in lines.iter().enumerate() {
            //let y_len = line.len() * 2 / 3;
            //let uv_len = line.len() / 3;

            tracing::debug!(?y, ?y_len, ?uv_len);

            for (mut x, luma) in line.iter().enumerate() {
                let luma = ((*luma - CHANNEL_LOW_TONE) / (CHANNEL_HIGH_TONE - CHANNEL_LOW_TONE))
                    .clamp(0.0, 1.0);
                let mut c = y % 2;

                if x > sync_porch_len {
                    x -= sync_porch_len;

                    if x < y_len {
                        yuv_buffer[x][c] = luma;
                    }
                    else {
                        x -= y_len;

                        if x > sep_len {
                            x -= sep_len;

                            if x < uv_len {
                                x *= 2;
                                c += 2;

                                yuv_buffer[x][c] = luma;
                                yuv_buffer[x + 1][c] = luma;
                            }
                        }
                    };
                }
            }

            if y % 2 == 1 {
                for (x, yuv) in yuv_buffer.iter().enumerate() {
                    // YCbCr -> RGB
                    let rgb_even: Rgb = YCbCr([yuv[0], yuv[3], yuv[2]]).into();
                    let rgb_odd: Rgb = YCbCr([yuv[1], yuv[3], yuv[2]]).into();

                    // B/W
                    //let rgb_even = Luma(yuv[0]).into();
                    //let rgb_odd = Luma(yuv[1]).into();

                    image.put_pixel(x as u32, (y - 1) as u32, rgb_even.into());
                    image.put_pixel(x as u32, y as u32, rgb_odd.into());
                }

                yuv_buffer.fill(Default::default());
            }
        }

        //play_audio(source, 0.5).await?;

        let image = DynamicImage::from(image);
        let image = image.resize_exact(
            ModeSpecification::R36.pixels_per_line as u32,
            image.height(),
            FilterType::Triangle,
        );

        Ok(image)
    }

    #[derive(Clone, Debug)]
    pub struct FrequencyCounter {
        //input_lowpass: MovingAverage<f32>,
        output_lowpass: MovingSum<f32>,
        state: f32,
        frequency_resolution: f32,
        threshold: f32,
    }

    impl FrequencyCounter {
        pub fn new(frequency_resolution: f32, sample_rate: f32) -> Self {
            // lowpass to remove DC
            //let input_lowpass = MovingAverage::from_cutoff_frequency(10.0,
            // sample_rate); tracing::debug!(n =
            // ?input_lowpass.len(), "dc block");

            // lowpass to retrieve a smooth signal from the zero-crossing
            // detection
            let lowpass_size = (sample_rate / frequency_resolution).floor() as usize;
            let output_lowpass = MovingSum::new(lowpass_size);
            tracing::debug!(size = ?lowpass_size, cutoff = output_lowpass.cutoff_frequency(sample_rate));

            Self {
                //input_lowpass,
                output_lowpass,
                state: 0.0,
                frequency_resolution,
                threshold: 0.01,
            }
        }
    }

    impl Scanner<f32> for FrequencyCounter {
        type Output = f32;

        fn scan(&mut self, sample: f32) -> Self::Output {
            //sample -= self.input_lowpass.scan(sample);

            let mut new_state = self.state;
            let mut pulse = 0.0;

            if self.state < 0.0 {
                if sample > self.threshold {
                    new_state = 1.0;
                    pulse = 1.0;
                }
            }
            else {
                if sample < -self.threshold {
                    new_state = -1.0;
                    //pulse = 1.0;
                }
            }
            self.state = new_state;

            (self.output_lowpass.scan(pulse) * self.frequency_resolution).round()
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub struct ToneDetect {
        frequency: f32,
        low_threshold: f32,
        high_threshold: f32,
        state: bool,
    }

    impl ToneDetect {
        pub fn new(frequency: f32, low_threshold: f32, high_threshold: f32) -> Self {
            Self {
                frequency,
                low_threshold,
                high_threshold,
                state: false,
            }
        }
    }

    impl Scanner<f32> for ToneDetect {
        type Output = (bool, Option<Edge>);

        #[inline]
        fn scan(&mut self, sample: f32) -> Self::Output {
            let mut edge = None;
            let distance = (sample - self.frequency).abs();

            if self.state {
                if distance > self.high_threshold {
                    self.state = false;
                    edge = Some(Edge::Falling);
                }
            }
            else {
                if distance < self.low_threshold {
                    self.state = true;
                    edge = Some(Edge::Rising);
                }
            }

            (self.state, edge)
        }
    }
}

#[allow(dead_code)]
mod attempt2 {
    use anyhow::Error;
    use image::DynamicImage;
    use mrrp_core::{
        sample::Complex,
        signal::{
            AsyncReadSamples,
            GetSampleRate,
        },
    };
    use mrrp_filter::{
        HilbertFilter,
        MovingAverage,
    };
    use mrrp_modem::fm::FmDemodulator;
    use mrrp_sstv::SYNC_TONE;
    use mrrp_util::signal::{
        AsyncReadSamplesExt,
        Scanner,
    };

    use crate::commands::test::sstv::{
        Edge,
        attempt1::ToneDetect,
    };

    pub async fn decode<S>(mut source: S) -> Result<DynamicImage, Error>
    where
        S: AsyncReadSamples<Sample = f32> + GetSampleRate + Unpin,
        Error: From<S::Error>,
    {
        let sample_rate = source.sample_rate();

        let mut hilbert_filter = HilbertFilter::new(sample_rate, 500.0, 17);
        let mut frequency_detect = FrequencyDetect::new(sample_rate);

        let mut sync_detect = ToneDetect::new(SYNC_TONE, 10.0, 50.0);
        let mut previous_sync_time = 0;
        let mut num_syncs = 0;

        let mut i = 0;
        while let Some(sample) = source.read_sample_or_eof().await? {
            let sample = hilbert_filter.scan(sample);
            let frequency = frequency_detect.scan(sample);

            let t = i as f32 / sample_rate;

            let (_sync_state, sync_edge) = sync_detect.scan(frequency);

            if let Some(Edge::Rising) = sync_edge {
                num_syncs += 1;

                println!(
                    "t={t:.3}s SYNC! f={frequency}, dt={:.3}ms, n={num_syncs}",
                    (i - previous_sync_time) as f32 / sample_rate * 1000.0
                );

                previous_sync_time = i;
            }

            i += 1;
        }

        todo!();
    }

    #[derive(Debug)]
    pub struct FrequencyDetect {
        fm_demod: FmDemodulator,
        lowpass: MovingAverage<f32>,
    }

    impl FrequencyDetect {
        pub fn new(sample_rate: f32) -> Self {
            Self {
                fm_demod: FmDemodulator::new(sample_rate, 1.0),
                lowpass: MovingAverage::new(100),
            }
        }
    }

    impl Scanner<Complex<f32>> for FrequencyDetect {
        type Output = f32;

        fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
            let frequency = self.fm_demod.scan(sample);
            self.lowpass.scan(frequency)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    Rising,
    Falling,
}
