use std::{
    convert::Infallible,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use num_complex::Complex;

use crate::{
    buf::SampleBufMut,
    io::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
        Remaining,
        StreamLength,
    },
    modem::sstv::{
        LEADER_BREAK_TIME,
        LEADER_TIME,
        LEADER_TONE,
        PORCH_TONE,
        SYNC_TONE,
        VIS_BIT_TIME,
        VIS_HIGH_TONE,
        VIS_LOW_TONE,
        image::FrameBuffer,
        modes::ModeSpecification,
        state::{
            HeaderState,
            LineState,
            State,
        },
    },
    source::{
        ComplexSinusoid,
        SignalGenerator,
    },
    util::lerp,
};

#[derive(Clone, Debug)]
pub struct SstvEncoder<F> {
    frame_buffer: F,
    mode: ModeSpecification,
    state: Option<(State, PulseGenerator)>,
    sample_rate: f32,
}

impl<F> SstvEncoder<F>
where
    F: FrameBuffer,
{
    pub fn new(frame_buffer: F, mode: ModeSpecification, sample_rate: f32) -> Self {
        assert_eq!(frame_buffer.width(), mode.pixels_per_line);
        assert_eq!(frame_buffer.height(), mode.num_lines);

        let state = State::default();
        let pulse = Pulse::from_state(&state, &mode, &frame_buffer);

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
                if let Some(next_state) = state.next(Some(&this.mode)) {
                    let next_pulse = Pulse::from_state(&next_state, &this.mode, &this.frame_buffer);

                    // this matches the phase of the new pulse with the phase of the old pulse. this
                    // reduces unwanted frequencies caused by the transition
                    pulse_generator.set_pulse(next_pulse);

                    *state = next_state;
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

#[derive(Clone, Copy, Debug)]
struct Pulse {
    pub frequency: f32,
    pub duration: f32,
}

impl Pulse {
    #[inline]
    pub fn new(frequency: f32, duration: f32) -> Self {
        Self {
            frequency,
            duration,
        }
    }

    pub fn from_state<F>(state: &State, mode: &ModeSpecification, frame_buffer: &F) -> Self
    where
        F: FrameBuffer,
    {
        match state {
            State::Header { header_state } => {
                match header_state {
                    HeaderState::Leader1 | HeaderState::Leader2 => {
                        Pulse::new(LEADER_TONE, LEADER_TIME)
                    }
                    HeaderState::LeaderBreak => Pulse::new(SYNC_TONE, LEADER_BREAK_TIME),
                    HeaderState::VisStart | HeaderState::VisStop => {
                        Pulse::new(SYNC_TONE, VIS_BIT_TIME)
                    }
                    HeaderState::VisBit { bit } => {
                        let bit = if *bit == 7 {
                            mode.vis_code.parity()
                        }
                        else {
                            mode.vis_code.get_bit(*bit)
                        };
                        Pulse::new(if bit { VIS_HIGH_TONE } else { VIS_LOW_TONE }, VIS_BIT_TIME)
                    }
                }
            }
            State::Line { y, line_state } => {
                match line_state {
                    LineState::Sync => Pulse::new(SYNC_TONE, mode.sync_time),
                    LineState::Porch => Pulse::new(PORCH_TONE, mode.porch_time),
                    LineState::Scan { channel, x } => {
                        let channel_value = frame_buffer.channel(*x, *y, *channel);
                        let channel_frequency = lerp(channel_value as f32 / 255.0, 1500.0, 2300.0);
                        Pulse::new(channel_frequency, mode.pixel_time)
                    }
                    LineState::Separator { channel: _ } => Pulse::new(PORCH_TONE, mode.sep_time),
                }
            }
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
