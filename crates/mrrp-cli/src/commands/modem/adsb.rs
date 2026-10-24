use anyhow::Error;
use clap::{
    Parser,
    ValueEnum,
};
use futures_util::TryStreamExt;
use mrrp_adsb::{
    decode::FrameWithChecksum,
    demod::{
        DemodulateStream,
        Demodulator,
        RawFrame,
    },
};
use mrrp_rtl_sdr::tuner::gain;
use mrrp_util::signal::AsyncReadSamplesExt;

use crate::{
    Context,
    commands::rtl_sdr::open::DeviceArgs,
    util::run_til_shutdown,
};

const SAMPLE_RATE: f32 = 2_000_000.0;
const CENTER_FREQUENCY: f32 = 1_090_000.0;

pub async fn run(args: Args, _context: Context) -> Result<(), Error> {
    // open device, configure and get sample stream

    let mut device = args.device.open_device().await?;

    // todo: these two should probably be put into a DeviceOptions struct that
    // can be shared with other commands that just let the user configure these.
    device.set_bias_t(args.bias_t).await?;
    if let Some(gain) = args.gain {
        device.set_tuner_gain(gain::Db(gain)).await?;
    }
    else {
        device.set_tuner_gain(gain::Auto).await?;
    }

    device.set_sample_rate(SAMPLE_RATE).await?;
    device.set_center_frequency(CENTER_FREQUENCY).await?;

    let reader = device.reader(Default::default()).await?.convert();

    // create adsb demodulator. the resulting stream returns raw frames (i.e. 2,
    // 7 or 14 bytes per item)

    let demodulator = Demodulator::new(args.quality.into(), args.allowed_errors);
    let mut frames = DemodulateStream::new(reader, demodulator, args.buffer_size);

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

                let frame = match FrameWithChecksum::decode(&mut frame.bytes()) {
                    Ok(frame) => {
                        // check CRC of frame if possible
                        match frame.check() {
                            Some(true) => {
                                // frame is valid
                                frame
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
                    Err(_error) => {
                        // just ignore errors as they're likely from our buggy
                        // demodulator
                        continue;
                    }
                };

                println!("{:#?}", frame.frame);
                println!("");
            }
        }
        Ok::<_, Error>(())
    })
    .await
    .transpose()?;

    device.close().await?;

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

    #[clap(short = 'Q', long, default_value = "one-bit")]
    quality: Quality,

    #[clap(short = 'e', long, default_value = "5")]
    allowed_errors: usize,

    #[clap(short, long, default_value = "65536")]
    buffer_size: usize,

    #[clap(long)]
    raw: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum Quality {
    NoChecks,
    HalfBit,
    OneBit,
    TwoBits,
}

impl From<Quality> for mrrp_adsb::demod::Quality {
    fn from(value: Quality) -> Self {
        match value {
            Quality::NoChecks => Self::NoChecks,
            Quality::HalfBit => Self::HalfBit,
            Quality::OneBit => Self::OneBit,
            Quality::TwoBits => Self::TwoBits,
        }
    }
}
