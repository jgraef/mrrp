mod modes;

use std::{
    convert::Infallible,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use image::RgbImage;
use num_complex::Complex;

pub use crate::modem::sstv::modes::ModeSpecification;
use crate::{
    buf::SampleBufMut,
    io::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
    },
    source::{
        ComplexSinusoid,
        SignalGenerator,
    },
};

pub const HEADER_LEADER_TONE: f32 = 1900.0;
pub const HEADER_LEADER_TIME: f32 = 0.300;

pub const HEADER_BREAK_TONE: f32 = 1200.0;
pub const HEADER_BREAK_TIME: f32 = 0.010;

pub const HEADER_VIS_BIT_TIME: f32 = 0.030;
pub const HEADER_VIS_START_STOP_TONE: f32 = 1200.0;
pub const HEADER_VIS_LOW_TONE: f32 = 1300.0;
pub const HEADER_VIS_HIGH_TONE: f32 = 1100.0;

#[derive(Clone, Copy, Debug)]
enum EncoderState {
    Leader1,
    LeaderBreak,
    Leader2,
    VisStart,
    VisBit {
        remaining_count: u8,
        remaining_bits: u8,
    },
    VisStop,
}

impl EncoderState {
    pub fn next(&self, mode: &ModeSpecification) -> Option<Self> {
        match self {
            Self::Leader1 => Some(Self::LeaderBreak),
            Self::LeaderBreak => Some(Self::Leader2),
            Self::Leader2 => Some(Self::VisStart),
            Self::VisStart => {
                Some(Self::VisBit {
                    remaining_count: 8,
                    remaining_bits: encode_with_parity(mode.vis_code),
                })
            }
            Self::VisBit {
                remaining_count,
                remaining_bits,
            } => {
                if *remaining_count > 1 {
                    Some(Self::VisBit {
                        remaining_count: *remaining_count - 1,
                        remaining_bits: *remaining_bits >> 1,
                    })
                }
                else {
                    Some(Self::VisStop)
                }
            }
            Self::VisStop => None,
        }
    }

    pub fn pulse(&self, sample_rate: f32) -> Pulse {
        match self {
            Self::Leader1 | Self::Leader2 => {
                Pulse::new(HEADER_LEADER_TONE, HEADER_LEADER_TIME, sample_rate)
            }
            Self::LeaderBreak => Pulse::new(HEADER_BREAK_TONE, HEADER_BREAK_TIME, sample_rate),
            Self::VisStart | Self::VisStop => {
                Pulse::new(HEADER_VIS_START_STOP_TONE, HEADER_VIS_BIT_TIME, sample_rate)
            }
            Self::VisBit {
                remaining_count: _,
                remaining_bits,
            } => {
                Pulse::new(
                    if *remaining_bits & 1 == 0 {
                        HEADER_VIS_LOW_TONE
                    }
                    else {
                        HEADER_VIS_HIGH_TONE
                    },
                    HEADER_VIS_BIT_TIME,
                    sample_rate,
                )
            }
        }
    }
}

fn encode_with_parity(vis_code: u8) -> u8 {
    assert!(vis_code & 0x80 == 0);
    let parity = (vis_code << 7)
        ^ (vis_code << 6)
        ^ (vis_code << 5)
        ^ (vis_code << 4)
        ^ (vis_code << 3)
        ^ (vis_code << 2)
        ^ (vis_code << 1);
    vis_code | (parity & 0x80)
}

#[derive(Clone, Copy, Debug)]
struct Pulse {
    sinusoid: ComplexSinusoid,
    samples_remaining: usize,
}

impl Pulse {
    pub fn new(frequency: f32, duration: f32, sample_rate: f32) -> Self {
        // todo: remove this
        const TEST_STRETCH: f32 = 1.0;

        Self {
            sinusoid: ComplexSinusoid::new(frequency, sample_rate),
            samples_remaining: (duration * sample_rate * TEST_STRETCH) as usize,
        }
    }

    pub fn next(&mut self) -> Option<Complex<f32>> {
        if self.samples_remaining > 0 {
            let sample = self.sinusoid.next();
            self.samples_remaining -= 1;
            Some(sample)
        }
        else {
            None
        }
    }
}

#[derive(Clone, Debug)]
pub struct SstvEncoder {
    _image: RgbImage,
    mode: ModeSpecification,
    state: Option<(EncoderState, Pulse)>,
    sample_rate: f32,
}

impl SstvEncoder {
    pub fn new(image: RgbImage, mode: ModeSpecification, sample_rate: f32) -> Self {
        let state = EncoderState::Leader1;

        Self {
            _image: image,
            mode,
            state: Some((state, state.pulse(sample_rate))),
            sample_rate,
        }
    }
}

impl AsyncReadSamples<Complex<f32>> for SstvEncoder {
    type Error = Infallible;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Complex<f32>>,
    ) -> Poll<Result<(), Self::Error>> {
        while buffer.has_remaining_mut() {
            let this = &mut *self;

            let Some((state, pulse)) = &mut this.state
            else {
                break;
            };
            if let Some(sample) = pulse.next() {
                buffer.put_sample(sample);
            }
            else {
                this.state = state
                    .next(&this.mode)
                    .map(|state| (state, state.pulse(this.sample_rate)));
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl GetSampleRate for SstvEncoder {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}
