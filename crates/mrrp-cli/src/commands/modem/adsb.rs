use std::{
    mem::MaybeUninit,
    pin::Pin,
    task::Poll,
};

use anyhow::Error;
use clap::Parser;
use futures_util::{
    Stream,
    TryStreamExt,
};
use mrrp_adsb::{
    decode::{
        DownlinkFormat,
        FrameWithChecksum,
    },
    demod::{
        Demodulator,
        RawFrame,
    },
};
use mrrp_audio::WavSource;
use mrrp_core::{
    buf::UninitSlice,
    sample::Complex,
    signal::{
        AsyncReadSamples,
        ReadBuf,
    },
};
use mrrp_util::signal::AsyncReadSamplesExt;
use pin_project_lite::pin_project;

use crate::{
    Context,
    commands::rtl_sdr::open::DeviceArgs,
    util::run_til_shutdown,
};

pub async fn run(args: Args, _context: Context) -> Result<(), Error> {
    // open device, configure and get sample stream

    /*let mut device = args.device.open_device().await?;

    // todo: these two should probably be put into a DeviceOptions struct that
    // can be shared with other commands that just let the user configure these.
    device.set_bias_t(args.bias_t).await?;
    if let Some(gain) = args.gain {
        device.set_tuner_gain(gain::Db(gain)).await?;
    }
    else {
        device.set_tuner_gain(gain::Auto).await?;
    }

    device.set_sample_rate(SAMPLE_RATE as f32).await?;
    device
        .set_center_frequency(DOWNLINK_FREQUENCY as f32)
        .await?;

    let reader = device
        .reader(Default::default())
        .await?
        .convert::<Complex<f32>>();*/

    let reader = WavSource::<_, Complex<i16>>::from_path(
        "tmp/baseband_1090000000Hz_03-42-05_25-10-2026.wav",
    )?
    .convert::<Complex<f32>>();

    // create adsb demodulator. the resulting stream returns raw frames (i.e. 2,
    // 7 or 14 bytes per item)
    let mut demodulator = Demodulator::default();
    let mut frames = MultiEmitter::new(
        reader,
        |sample: Complex<f32>, output: &mut Vec<RawFrame>| {
            demodulator.push(sample.norm(), output);
        },
    );

    run_til_shutdown(async move {
        if args.raw {
            // print raw frames

            while let Some(frame) = frames.try_next().await? {
                let ty = match frame {
                    RawFrame::ModeAc { data: _ } => "ac",
                    RawFrame::ModeSShort { data: _ } => "s (short)",
                    RawFrame::ModeSLong { data: _ } => "s (long)",
                };

                println!("mode-{ty}: {}", const_hex::display(frame.bytes()));
            }
        }
        else {
            while let Some(frame) = frames.try_next().await? {
                // we only decode mode-s frames. we believe mode-ac only
                // contains barometer and squawk.
                if !frame.is_mode_s() {
                    continue;
                }

                let Ok(frame) = FrameWithChecksum::decode(&mut frame.bytes())
                else {
                    // just ignore errors as they're likely from our buggy
                    // demodulator
                    continue;
                };

                if !args.no_crc {
                    // check CRC of frame if possible
                    match frame.check() {
                        Some(true) => {
                            // frame is valid
                        }
                        Some(false) => {
                            // frame is invalid. this is likely because we
                            // demodulated something with errors. we can
                            // jsut ignore
                            // this.
                            continue;
                        }

                        None => {
                            // can't check this frame via CRC. since only
                            // low-quality demod works right now, we'll
                            // ignore those.
                            continue;
                        }
                    }
                }

                let show = match frame.frame.downlink_format() {
                    DownlinkFormat::ShortAirAirSurveillance => true,
                    DownlinkFormat::SurveillanceAltitudeReply => true,
                    DownlinkFormat::SurveillanceIdentityReply => true,
                    DownlinkFormat::AllCallReply => true,
                    DownlinkFormat::LongAirAirSurveillance => true,
                    DownlinkFormat::ExtendedSquitter => true,
                    DownlinkFormat::ExtendedSquitterNonTransponder => true,
                    DownlinkFormat::MilitaryExtendedSquitter => false,
                    DownlinkFormat::CommBAltitudeReply => true,
                    DownlinkFormat::CommBIdentityReply => true,
                    DownlinkFormat::MilitaryUse => false,
                    DownlinkFormat::CommD => false,
                };

                if show {
                    println!("{:#?}", frame.frame);
                    println!("");
                }
            }
        }
        Ok::<_, Error>(())
    })
    .await
    .transpose()?;

    //device.close().await?;

    Ok(())
}

#[derive(Clone, Debug, Parser)]
pub struct Args {
    #[clap(flatten)]
    device: DeviceArgs,

    #[clap(short = 'b', long)]
    bias_t: bool,

    #[clap(short = 'g', long)]
    gain: Option<f32>,

    #[clap(short, long, default_value = "65536")]
    buffer_size: usize,

    #[clap(long)]
    raw: bool,

    // todo: allow to specify how to handle frames that we can't check
    #[clap(long)]
    no_crc: bool,
}

pin_project! {
    /// Filters a [`AsyncReadSamples`] into a [`Stream`] by applying a callback.
    #[derive(Debug)]
    pub struct MultiEmitter<R, F,T> {
        #[pin]
        pub inner: R,
        f: F,
        buffer: Vec<T>,
    }
}

impl<R, F, T> MultiEmitter<R, F, T> {
    pub fn new(inner: R, f: F) -> Self {
        Self {
            inner,
            f,
            buffer: Vec::new(),
        }
    }
}

impl<R, F, T> Stream for MultiEmitter<R, F, T>
where
    R: AsyncReadSamples,
    // todo: if we make this a proper trait, we make it take an impl Extend as output
    F: FnMut(R::Sample, &mut Vec<T>),
{
    type Item = Result<T, R::Error>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        // todo: use buffer?

        let mut this = self.project();

        let mut buffer = [MaybeUninit::uninit(); 1];

        loop {
            if let Some(item) = this.buffer.pop() {
                return Poll::Ready(Some(Ok(item)));
            }

            let mut read_buf = ReadBuf::uninit(UninitSlice::slice_mut_from_uninit(&mut buffer));
            match this.inner.as_mut().poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(Some(Err(error)));
                }
                Poll::Ready(Ok(())) => {
                    let n = read_buf.filled().len();

                    if n == 0 {
                        // end of stream
                        return Poll::Ready(None);
                    }
                    else {
                        assert!(n == 1);
                        let sample = unsafe {
                            // SAFETY: The buffer had one element written to it,
                            // so this must be initialized.
                            MaybeUninit::assume_init_read(&buffer[0])
                        };

                        // todo: make self.buffer a VecDeque. we don't right now
                        // because we'd need to have the Fn take a VecDeque,
                        // which is suboptimal. Would be better if we made f
                        // impl a trait and have it accept any impl Extend<T>
                        (this.f)(sample, &mut this.buffer);
                        this.buffer.reverse();
                    }
                }
            }
        }
    }
}
