use std::{
    cmp::Ordering,
    pin::Pin,
    task::{
        Context,
        Poll,
        ready,
    },
};

use num_complex::Complex;
use pin_project_lite::pin_project;

use crate::{
    filter::GoertzelFilter,
    io::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
        combinators::Scanner,
    },
    modem::{
        fm,
        sstv::{
            CHANNEL_HIGH_TONE,
            CHANNEL_LOW_TONE,
            LEADER_TONE,
            SYNC_TONE,
            VIS_BIT_TIME,
            VIS_HIGH_TONE,
            VIS_LOW_TONE,
            image::FrameBufferMut,
            modes::{
                DefaultModes,
                ModeSelectError,
                ModeSpecification,
                SelectMode,
                VisCode,
            },
            state::{
                HeaderState,
                LineState,
                State,
            },
        },
    },
    util::unlerp,
};

#[derive(Clone, Copy, Debug)]
struct EdgeDetect {
    goertzel: GoertzelFilter,
    delay: f32,
    trigger_threshold: f32,
}

impl EdgeDetect {
    pub fn new(
        sample_rate: f32,
        trigger_frequency: f32,
        trigger_bandwidth: f32,
        trigger_threshold: f32,
    ) -> Self {
        Self {
            goertzel: GoertzelFilter::new(sample_rate, trigger_frequency, trigger_bandwidth),
            delay: 0.0,
            trigger_threshold,
        }
    }
}

impl Scanner<Complex<f32>> for EdgeDetect {
    type Output = Option<Edge>;

    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        let value = self.goertzel.scan(sample).norm();
        let delta = value - self.delay;
        self.delay = value;
        if delta > self.trigger_threshold {
            Some(Edge::Rising)
        }
        else if delta < -self.trigger_threshold {
            Some(Edge::Falling)
        }
        else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Rising,
    Falling,
}

#[derive(Clone, Copy, Debug)]
struct VisBitDetect {
    goertzel_low: GoertzelFilter,
    goertzel_high: GoertzelFilter,
    trigger_threshold: f32,
}

impl VisBitDetect {
    pub fn new(
        sample_rate: f32,
        trigger_frequency_low: f32,
        trigger_frequency_high: f32,
        trigger_bandwidth: f32,
        trigger_threshold: f32,
    ) -> Self {
        Self {
            goertzel_low: GoertzelFilter::new(
                sample_rate,
                trigger_frequency_low,
                trigger_bandwidth,
            ),
            goertzel_high: GoertzelFilter::new(
                sample_rate,
                trigger_frequency_high,
                trigger_bandwidth,
            ),
            trigger_threshold,
        }
    }
}

impl Scanner<Complex<f32>> for VisBitDetect {
    type Output = Option<bool>;

    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        let value_low = self.goertzel_low.scan(sample).norm();
        let value_high = self.goertzel_high.scan(sample).norm();
        if value_low > value_high + self.trigger_threshold {
            Some(false)
        }
        else if value_high > value_low + self.trigger_threshold {
            Some(true)
        }
        else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct ChannelSample {
    frequency: fm::DifferentiateAndDivide,
    frequency_low: f32,
    frequency_high: f32,
}

impl ChannelSample {
    pub fn new(sample_rate: f32, frequency_low: f32, frequency_high: f32) -> Self {
        Self {
            frequency: fm::DifferentiateAndDivide::new(sample_rate, 1.0),
            frequency_low,
            frequency_high,
        }
    }
}

impl Scanner<Complex<f32>> for ChannelSample {
    type Output = f32;

    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        let frequency = self.frequency.scan(sample);
        unlerp(frequency, self.frequency_low, self.frequency_high).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug)]
struct Filters {
    leader: EdgeDetect,
    sync: EdgeDetect,
    vis: VisBitDetect,
    channel: ChannelSample,
}

impl Filters {
    pub fn new(sample_rate: f32) -> Self {
        let leader = EdgeDetect::new(sample_rate, LEADER_TONE, 100.0, 0.1);
        let sync = EdgeDetect::new(sample_rate, SYNC_TONE, 50.0, 0.5);
        let vis = VisBitDetect::new(sample_rate, VIS_LOW_TONE, VIS_HIGH_TONE, 100.0, 0.1);
        let channel = ChannelSample::new(sample_rate, CHANNEL_LOW_TONE, CHANNEL_HIGH_TONE);

        Self {
            leader,
            sync,
            vis,
            channel,
        }
    }
}

pin_project! {
    #[derive(Clone, Debug)]
    pub struct SstvDecoder<R, F, M = DefaultModes> {
        #[pin]
        input: R,
        sample_rate: f32,
        samples_consumed: usize,
        filters: Filters,
        state: Option<(State, PulseAcceptor)>,
        frame_buffer: F,
        vis_code: u8,
        mode: Option<ModeSpecification>,
        select_mode: M,
    }
}

impl<R, F> SstvDecoder<R, F, DefaultModes>
where
    R: GetSampleRate,
{
    #[inline]
    pub fn new(input: R, frame_buffer: F) -> Self {
        Self::new_with_mode_select(input, frame_buffer, DefaultModes)
    }
}

impl<R, F, M> SstvDecoder<R, F, M>
where
    R: GetSampleRate,
    M: SelectMode,
{
    pub fn new_with_mode_select(input: R, frame_buffer: F, select_mode: M) -> Self {
        let sample_rate = input.sample_rate();
        let filters = Filters::new(sample_rate);

        let state = State::default();
        let pulse_acceptor = PulseAcceptor::from_state(&state, sample_rate, None);

        Self {
            input,
            sample_rate,
            samples_consumed: 0,
            filters,
            state: Some((state, pulse_acceptor)),
            frame_buffer,
            vis_code: 0,
            mode: None,
            select_mode,
        }
    }
}

impl<R, F, M> Future for SstvDecoder<R, F, M>
where
    R: AsyncReadSamples<Complex<f32>>,
    F: FrameBufferMut,
    M: SelectMode,
{
    type Output = Result<(), DecodeError<R::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let this = self.as_mut().project();

            let Some((state, pulse_acceptor)) = this.state
            else {
                return Poll::Ready(Ok(()));
            };
            let mut state = state;

            let sample = {
                // for now we'll just read one sample at a time
                let mut samples: [_; 1] = Default::default();
                let mut read_buf = ReadBuf::new(&mut samples[..]);
                ready!(this.input.poll_read_samples(cx, &mut read_buf))
                    .map_err(DecodeError::Stream)?;
                if read_buf.filled().len() == 0 {
                    return Poll::Ready(Err(DecodeError::Eof));
                }
                *this.samples_consumed += 1;
                samples[0]
            };

            match pulse_acceptor.accept_sample(sample, this.filters) {
                Poll::Pending => {}
                Poll::Ready(accepted_pulse) => {
                    tracing::debug!(t=?this.samples_consumed, ?accepted_pulse, ?state, "accepted pulse");

                    let mut skip_to = None;

                    match (accepted_pulse, &mut state) {
                        (AcceptedPulse::Leader { length }, _) => {
                            let length_in_seconds = length as f32 / *this.sample_rate;
                            tracing::debug!(?length, length_in_seconds, "leader")
                        }
                        (
                            AcceptedPulse::VisBit { bit: bit_value },
                            State::Header {
                                header_state: HeaderState::VisBit { bit },
                            },
                        ) => {
                            let Some(bit_value) = bit_value
                            else {
                                return Poll::Ready(Err(DecodeError::InvalidVis));
                            };

                            if *bit == 7 {
                                // parity bit

                                let vis_code = VisCode::new(*this.vis_code).unwrap();
                                let mode = this
                                    .select_mode
                                    .mode_specification_with_parity(vis_code, bit_value)?;
                                tracing::debug!(mode = %mode.name);

                                *this.mode = Some(mode);
                                this.frame_buffer
                                    .set_size(mode.pixels_per_line, mode.num_lines);
                            }
                            else if bit_value {
                                *this.vis_code |= 1 << *bit;
                            }
                        }
                        (
                            AcceptedPulse::Channel { value },
                            State::Line {
                                y,
                                line_state: LineState::Scan { channel, x },
                            },
                        ) => {
                            let value = (value * 255.0).clamp(0.0, 255.0) as u8;
                            this.frame_buffer.set_channel(*x, *y, *channel, value);
                        }
                        (
                            AcceptedPulse::Sync,
                            State::Line {
                                y,
                                line_state: LineState::Scan { channel, x },
                            },
                        ) => {
                            tracing::warn!(?channel, ?x, "early sync pulse");

                            let mode = this.mode.as_ref().unwrap();
                            if *y + 1 == mode.num_lines {
                                *this.state = None;
                                return Poll::Ready(Ok(()));
                            }
                            else {
                                skip_to = Some(State::Line {
                                    y: *y + 1,
                                    line_state: LineState::Sync,
                                });
                            };
                        }

                        _ => {}
                    }

                    if let Some(next_state) =
                        skip_to.take().or_else(|| state.next(this.mode.as_ref()))
                    {
                        let pulse_acceptor = PulseAcceptor::from_state(
                            &next_state,
                            *this.sample_rate,
                            this.mode.as_ref(),
                        );
                        *this.state = Some((next_state, pulse_acceptor));
                    }
                    else {
                        *this.state = None;
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug, thiserror::Error)]
#[error("sstv decoder error")]
pub enum DecodeError<S> {
    Stream(S),
    Eof,
    InvalidVis,
    ModeSelect(#[from] ModeSelectError),
}

#[derive(Clone, Copy, Debug)]
enum PulseAcceptor {
    Leader {
        locked: bool,
        length: usize,
    },
    Sync,
    Porch {
        remaining: usize,
    },
    VisBit {
        low_votes: usize,
        high_votes: usize,
        remaining: usize,
    },
    Scan {
        remaining: usize,
        sample_sum: f32,
        num_samples: usize,
    },
}

impl PulseAcceptor {
    pub fn from_state(state: &State, sample_rate: f32, mode: Option<&ModeSpecification>) -> Self {
        match state {
            State::Header { header_state } => {
                match header_state {
                    HeaderState::Leader1 | HeaderState::Leader2 => {
                        Self::Leader {
                            locked: false,
                            length: 0,
                        }
                    }
                    HeaderState::LeaderBreak | HeaderState::VisStart | HeaderState::VisStop => {
                        Self::Sync
                    }
                    HeaderState::VisBit { bit: _ } => {
                        let remaining = (VIS_BIT_TIME * sample_rate) as usize;
                        tracing::debug!("vis bit samples: {remaining}");
                        Self::VisBit {
                            low_votes: 0,
                            high_votes: 0,
                            remaining,
                        }
                    }
                }
            }
            State::Line { y: _, line_state } => {
                let mode = mode.expect("expected mode specification in line state");
                match line_state {
                    LineState::Sync => Self::Sync,
                    LineState::Porch => {
                        let remaining = (mode.porch_time * sample_rate) as usize;
                        Self::Porch { remaining }
                    }
                    LineState::Scan { channel: _, x: _ } => {
                        let remaining = (mode.pixel_time * sample_rate) as usize;
                        Self::Scan {
                            remaining,
                            sample_sum: 0.0,
                            num_samples: remaining,
                        }
                    }
                    LineState::Separator { channel: _ } => {
                        let remaining = (mode.sep_time * sample_rate) as usize;
                        Self::Porch { remaining }
                    }
                }
            }
        }
    }

    pub fn accept_sample(
        &mut self,
        sample: Complex<f32>,
        filters: &mut Filters,
    ) -> Poll<AcceptedPulse> {
        match self {
            PulseAcceptor::Leader { locked, length } => {
                if *locked {
                    *length += 1;
                }

                match filters.leader.scan(sample) {
                    None => {}
                    Some(Edge::Rising) => {
                        *locked = true;
                        *length = 0;
                    }
                    Some(Edge::Falling) => {
                        return Poll::Ready(AcceptedPulse::Leader { length: *length });
                    }
                }
            }
            PulseAcceptor::Sync => {
                match filters.sync.scan(sample) {
                    Some(Edge::Falling) => return Poll::Ready(AcceptedPulse::Sync),
                    _ => {}
                }
            }
            PulseAcceptor::Porch { remaining } => {
                // note: we can't use edge-detect here, since the following signal might have
                // the same frequency

                /*match filters.porch.scan(sample) {
                    Some(Edge::Falling) => return Poll::Ready(AcceptedPulse::Porch),
                    _ => {}
                }*/

                match filters.sync.scan(sample) {
                    Some(Edge::Rising) => {
                        // todo: stop line scan
                        todo!("rising sync pulse in porch");
                    }
                    _ => {}
                }

                *remaining -= 1;
                if *remaining == 0 {
                    return Poll::Ready(AcceptedPulse::Porch);
                }
            }
            PulseAcceptor::VisBit {
                low_votes,
                high_votes,
                remaining,
            } => {
                match filters.vis.scan(sample) {
                    None => {}
                    Some(false) => *low_votes += 1,
                    Some(true) => *high_votes += 1,
                }

                *remaining -= 1;
                if *remaining == 0 {
                    let bit = match low_votes.cmp(&high_votes) {
                        Ordering::Less => Some(true),
                        Ordering::Equal => None,
                        Ordering::Greater => Some(false),
                    };
                    return Poll::Ready(AcceptedPulse::VisBit { bit });
                }
            }
            PulseAcceptor::Scan {
                remaining,
                sample_sum,
                num_samples,
            } => {
                match filters.sync.scan(sample) {
                    Some(Edge::Rising) => {
                        // todo: stop line scan
                        //todo!("rising sync pulse");
                        return Poll::Ready(AcceptedPulse::Sync);
                    }
                    _ => {}
                }

                *sample_sum += filters.channel.scan(sample);
                *remaining -= 1;

                if *remaining == 0 {
                    return Poll::Ready(AcceptedPulse::Channel {
                        value: *sample_sum / *num_samples as f32,
                    });
                }
            }
        }

        Poll::Pending
    }
}

#[derive(Clone, Copy, Debug)]
enum AcceptedPulse {
    Leader { length: usize },
    Sync,
    Porch,
    VisBit { bit: Option<bool> },
    Channel { value: f32 },
}
