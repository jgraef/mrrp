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
        Remaining,
        StreamLength,
    },
    source::{
        ComplexSinusoid,
        SignalGenerator,
    },
    util::lerp,
};

pub const HEADER_LEADER_TONE: f32 = 1900.0;
pub const HEADER_LEADER_TIME: f32 = 0.300;

pub const HEADER_BREAK_TONE: f32 = 1200.0;
pub const HEADER_BREAK_TIME: f32 = 0.010;

pub const HEADER_VIS_BIT_TIME: f32 = 0.030;
pub const HEADER_VIS_START_STOP_TONE: f32 = 1200.0;
pub const HEADER_VIS_LOW_TONE: f32 = 1300.0;
pub const HEADER_VIS_HIGH_TONE: f32 = 1100.0;

pub const LINE_SYNC_TONE: f32 = 1200.0;
pub const LINE_PORCH_TONE: f32 = 1500.0;

pub trait FrameBuffer {
    fn width(&self) -> usize;
    fn height(&self) -> usize;
    fn channel(&self, x: usize, y: usize, channel: Channel) -> u8;
}

impl<F> FrameBuffer for &F
where
    F: FrameBuffer,
{
    #[inline]
    fn width(&self) -> usize {
        (&**self).width()
    }

    #[inline]
    fn height(&self) -> usize {
        (&**self).height()
    }

    #[inline]
    fn channel(&self, x: usize, y: usize, channel: Channel) -> u8 {
        (&**self).channel(x, y, channel)
    }
}

impl FrameBuffer for RgbImage {
    #[inline]
    fn width(&self) -> usize {
        RgbImage::width(self).try_into().unwrap()
    }

    #[inline]
    fn height(&self) -> usize {
        RgbImage::height(self).try_into().unwrap()
    }

    #[inline]
    fn channel(&self, x: usize, y: usize, channel: Channel) -> u8 {
        let pixel = self.get_pixel(x.try_into().unwrap(), y.try_into().unwrap());
        match channel {
            Channel::Green => pixel.0[1],
            Channel::Blue => pixel.0[2],
            Channel::Red => pixel.0[0],
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum EncoderHeaderState {
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

#[derive(Clone, Copy, Debug)]
enum EncoderLineState {
    Sync,
    SyncPorch,
    Scan { channel: Channel, x: usize },
    Separator { channel: Channel },
}

#[derive(Clone, Copy, Debug, Default)]
pub enum Channel {
    #[default]
    Green,
    Blue,
    Red,
}

impl Channel {
    pub fn next(self) -> Option<Self> {
        match self {
            Channel::Green => Some(Self::Blue),
            Channel::Blue => Some(Self::Red),
            Channel::Red => None,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum EncoderState {
    Header {
        header_state: EncoderHeaderState,
    },
    Line {
        y: usize,
        line_state: EncoderLineState,
    },
}

impl Default for EncoderState {
    fn default() -> Self {
        EncoderState::Header {
            header_state: EncoderHeaderState::Leader1,
        }
    }
}

impl EncoderState {
    pub fn next(&self, mode: &ModeSpecification) -> Option<Self> {
        let mut state = *self;
        match &mut state {
            Self::Header { header_state } => {
                match header_state {
                    EncoderHeaderState::Leader1 => *header_state = EncoderHeaderState::LeaderBreak,
                    EncoderHeaderState::LeaderBreak => *header_state = EncoderHeaderState::Leader2,
                    EncoderHeaderState::Leader2 => *header_state = EncoderHeaderState::VisStart,
                    EncoderHeaderState::VisStart => {
                        *header_state = EncoderHeaderState::VisBit {
                            remaining_count: 8,
                            remaining_bits: encode_vis_code_with_parity(mode.vis_code),
                        };
                    }
                    EncoderHeaderState::VisBit {
                        remaining_count,
                        remaining_bits,
                    } => {
                        if *remaining_count > 1 {
                            *remaining_count -= 1;
                            *remaining_bits >>= 1;
                        }
                        else {
                            *header_state = EncoderHeaderState::VisStop;
                        }
                    }
                    EncoderHeaderState::VisStop => {
                        state = EncoderState::Line {
                            y: 0,
                            line_state: EncoderLineState::Sync,
                        }
                    }
                }
            }
            Self::Line { y, line_state } => {
                match line_state {
                    EncoderLineState::Sync => {
                        *line_state = EncoderLineState::SyncPorch;
                    }
                    EncoderLineState::SyncPorch => {
                        *line_state = EncoderLineState::Scan {
                            channel: Channel::default(),
                            x: 0,
                        }
                    }
                    EncoderLineState::Scan { channel, x } => {
                        *x += 1;
                        if *x == mode.pixels_per_line {
                            *line_state = EncoderLineState::Separator { channel: *channel };
                        }
                    }
                    EncoderLineState::Separator { channel } => {
                        if let Some(channel) = channel.next() {
                            *line_state = EncoderLineState::Scan { channel, x: 0 }
                        }
                        else {
                            *y += 1;
                            if *y == mode.num_lines {
                                return None;
                            }
                            *line_state = EncoderLineState::Sync;
                        }
                    }
                }
            }
        }

        Some(state)
    }

    pub fn pulse<F: FrameBuffer>(&self, mode: &ModeSpecification, frame_buffer: &F) -> Pulse {
        match self {
            EncoderState::Header { header_state } => {
                match header_state {
                    EncoderHeaderState::Leader1 | EncoderHeaderState::Leader2 => {
                        Pulse::new(HEADER_LEADER_TONE, HEADER_LEADER_TIME)
                    }
                    EncoderHeaderState::LeaderBreak => {
                        Pulse::new(HEADER_BREAK_TONE, HEADER_BREAK_TIME)
                    }
                    EncoderHeaderState::VisStart | EncoderHeaderState::VisStop => {
                        Pulse::new(HEADER_VIS_START_STOP_TONE, HEADER_VIS_BIT_TIME)
                    }
                    EncoderHeaderState::VisBit {
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
                        )
                    }
                }
            }
            EncoderState::Line { y, line_state } => {
                match line_state {
                    EncoderLineState::Sync => Pulse::new(LINE_SYNC_TONE, mode.sync_time),
                    EncoderLineState::SyncPorch => Pulse::new(LINE_PORCH_TONE, mode.porch_time),
                    EncoderLineState::Scan { channel, x } => {
                        let channel_value = frame_buffer.channel(*x, *y, *channel);
                        let channel_frequency = lerp(channel_value as f32 / 255.0, 1500.0, 2300.0);
                        Pulse::new(channel_frequency, mode.pixel_time)
                    }
                    EncoderLineState::Separator { channel: _ } => {
                        Pulse::new(LINE_PORCH_TONE, mode.sep_time)
                    }
                }
            }
        }
    }
}

pub fn encode_vis_code_with_parity(vis_code: u8) -> u8 {
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
    frequency: f32,
    duration: f32,
}

impl Pulse {
    #[inline]
    pub fn new(frequency: f32, duration: f32) -> Self {
        Self {
            frequency,
            duration,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PulseGenerator {
    sinusoid: ComplexSinusoid,
    samples_remaining: usize,
}

impl PulseGenerator {
    #[inline]
    pub fn from_pulse(pulse: Pulse, sample_rate: f32) -> Self {
        Self {
            sinusoid: ComplexSinusoid::new(pulse.frequency, sample_rate),
            samples_remaining: (pulse.duration * sample_rate) as usize,
        }
    }

    #[inline]
    pub fn set_pulse(&mut self, pulse: Pulse) {
        self.sinusoid.set_frequency(pulse.frequency);
        self.samples_remaining = (pulse.duration * self.sinusoid.sample_rate()) as usize;
    }

    #[inline]
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
pub struct SstvEncoder<F> {
    frame_buffer: F,
    mode: ModeSpecification,
    state: Option<(EncoderState, PulseGenerator)>,
    sample_rate: f32,
}

impl<F> SstvEncoder<F>
where
    F: FrameBuffer,
{
    pub fn new(frame_buffer: F, mode: ModeSpecification, sample_rate: f32) -> Self {
        assert_eq!(frame_buffer.width(), mode.pixels_per_line);
        assert_eq!(frame_buffer.height(), mode.num_lines);

        let state = EncoderState::default();
        let pulse = state.pulse(&mode, &frame_buffer);

        Self {
            frame_buffer,
            mode,
            state: Some((state, PulseGenerator::from_pulse(pulse, sample_rate))),
            sample_rate,
        }
    }
}

impl<F> AsyncReadSamples<Complex<f32>> for SstvEncoder<F>
where
    F: FrameBuffer + Unpin,
{
    type Error = Infallible;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Complex<f32>>,
    ) -> Poll<Result<(), Self::Error>> {
        while buffer.has_remaining_mut() {
            let this = &mut *self;

            let Some((state, pulse_generator)) = &mut this.state
            else {
                break;
            };

            if let Some(sample) = pulse_generator.next() {
                buffer.put_sample(sample);
            }
            else {
                if let Some(new_state) = state.next(&this.mode) {
                    let new_pulse = new_state.pulse(&this.mode, &this.frame_buffer);

                    // this matches the phase of the new pulse with the phase of the old pulse. this
                    // reduces unwanted frequencies caused by the transition
                    pulse_generator.set_pulse(new_pulse);

                    *state = new_state;
                }
                else {
                    this.state = None;
                }
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl<F> GetSampleRate for SstvEncoder<F> {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

impl<F> StreamLength for SstvEncoder<F> {
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }
}
