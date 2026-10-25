//! 1090 MHz Mode-S demodulator
//!
//! Uplink uses [DPSK][1] (not implemented), Downlink uses [BPPM][3].
//!
//! # Downlink
//!
//! Pulses are 0.5µs. First a preamble with 3 pulses separated by 1, 3.5 and 4.5
//! µs. TODO: What's the total length? The source says 8µs, but that doesn't
//! make sense as it must be 0.5 + 1 + 0.5 + 3.5 + 0.5 + 4.5 + 0.5
//!
//! # Sources
//!
//! - [Mode S - Differential Phase shift Keying (DPSK)][1]
//! - [Mode S Reply Encoding][2]
//! - [Transition to ADS-B][3]
//!
//! [1]: https://web.archive.org/web/20191115224604/https://www.radartutorial.eu/13.ssr/sr23.en.html
//! [2]: https://web.archive.org/web/20191115232412/https://www.radartutorial.eu/13.ssr/sr24.en.html
//! [3]: https://en.wikipedia.org/wiki/Pulse-position_modulation
//! [4]: http://www.aircraft-avoid.com/ads-b-transition.html

use std::{
    fmt::Debug,
    ops::Deref,
    task::Poll,
};

use crate::decode::{
    LENGTH_LONG,
    LENGTH_SHORT,
};

/// Sample rate: 2 samples/µs
pub const SAMPLE_RATE: u32 = 2_000_000;

/// Mode S downlink frequency: 1090 MHz
pub const DOWNLINK_FREQUENCY: u32 = 1_090_000_000;

/// Mode S uplink frequency: 1030 MHz
pub const UPLINK_FREQUENCY: u32 = 1_030_000_000;

/// Preamble length in samples. 8 µs / 16 samples
const PREAMBLE_SAMPLES: usize = 16;

/// Expected preamble edges. None if there's no edge for that sample.
const PREAMBLE_EDGES: [Option<Edge>; PREAMBLE_SAMPLES] = [
    None,
    Some(Edge::Falling),
    Some(Edge::Rising),
    Some(Edge::Falling),
    None,
    None,
    None,
    Some(Edge::Rising),
    Some(Edge::Falling),
    Some(Edge::Rising),
    Some(Edge::Falling),
    None,
    None,
    None,
    None,
    None,
];

const DATA_SAMPLES_SHORT: usize = LENGTH_SHORT * 8 * 2;
const DATA_SAMPLES_LONG: usize = LENGTH_LONG * 8 * 2;

const FRAME_MAX_SAMPLES: usize = PREAMBLE_SAMPLES + DATA_SAMPLES_LONG;

/// A raw data frame.
///
/// This only contains the demodulated bytes. Depending on the type of frame
/// these are 2, 7 or 14 bytes long.
///
/// Since they have a fixed and rather short
/// length, this can easily be stored on the stack, and no `Vec` is needed for
/// the variable-length byte array.
#[derive(Clone, Copy)]
pub enum RawFrame {
    ModeAc { data: [u8; 2] },
    ModeSShort { data: [u8; 7] },
    ModeSLong { data: [u8; 14] },
}

impl RawFrame {
    /// Returns the frame's bytes as slice
    #[inline]
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::ModeAc { data } => &data[..],
            Self::ModeSShort { data } => &data[..],
            Self::ModeSLong { data } => &data[..],
        }
    }

    #[inline]
    pub fn is_mode_ac(&self) -> bool {
        matches!(self, Self::ModeAc { data: _ })
    }

    #[inline]
    pub fn is_mode_s_short(&self) -> bool {
        matches!(self, Self::ModeSShort { data: _ })
    }

    #[inline]
    pub fn is_mode_s_long(&self) -> bool {
        matches!(self, Self::ModeSLong { data: _ })
    }

    #[inline]
    pub fn is_mode_s(&self) -> bool {
        matches!(
            self,
            Self::ModeSShort { data: _ } | Self::ModeSLong { data: _ }
        )
    }
}

impl AsRef<[u8]> for RawFrame {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.bytes()
    }
}

impl Deref for RawFrame {
    type Target = [u8];

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.bytes()
    }
}

impl Debug for RawFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (name, bytes) = match self {
            RawFrame::ModeAc { data } => ("ModeAc", &data[..]),
            RawFrame::ModeSShort { data } => ("ModeSShort", &data[..]),
            RawFrame::ModeSLong { data } => ("ModeSLong", &data[..]),
        };
        write!(f, "{name}({})", const_hex::display(bytes))
    }
}

#[derive(Clone, Debug)]
pub struct Demodulator {
    previous_sample: f32,

    // todo: use limited stack-allocated deque
    states: Vec<State>,
}

impl Default for Demodulator {
    fn default() -> Self {
        Self {
            // just use 0.0, instead of None. If we used None we would need to check in each push.
            // Using 0.0 we'll potentially create a single edge where there isn't one, which is
            // acceptable.
            previous_sample: 0.0,
            states: Vec::with_capacity(FRAME_MAX_SAMPLES),
        }
    }
}

impl Demodulator {
    pub fn push(&mut self, sample: f32, output: &mut Vec<RawFrame>) {
        // there will be at most one demodulator state per sample - usually much
        // fewer because most will fail early
        assert!(self.states.len() < FRAME_MAX_SAMPLES);

        let edge = Edge::from_samples([self.previous_sample, sample]);
        self.previous_sample = sample;

        // create new state for current sample
        self.states.push(State::default());

        // update existing demodulator states with current sample
        self.states.retain_mut(|state| {
            match state.push(edge) {
                Poll::Pending => {
                    // state not done, keep it
                    true
                }
                Poll::Ready(Ok(frame)) => {
                    // state is done and returned a valid frame

                    // note: only one state can finish at a time.
                    //
                    // if we only had frames with the same length, this would
                    // trivially follow from the fact that we start one state
                    // per sample - so they can only successfully finish after a
                    // fixed period and thus one per sample.
                    //
                    // this still holds with the 2 lengths of frames, since we
                    // can't have a long frame and a short frame overlapping
                    // such that they end at the same sample. this is because
                    // the short frame's preamble would fall into the long
                    // frames data. preambles contain sample pairs (i.e.
                    // silence) that cannot occur in the data.
                    //
                    // important! take care to handle this correctly if we allow
                    // for small errors. then we can very much have multiple
                    // states finish successfully at the same sample.
                    //
                    // this panicked on the first try :sweaty smile:. the
                    // preamble matching that we wrote is a lot more permissive
                    // and this can happen now. we'll have to figure out a
                    // better way to do this `Emitter` thing.
                    //
                    //assert!(output.is_none(), "Only one demodulator state can
                    // finish successfully at the same sample. If this is not
                    // the case anymore read the comment here.");

                    output.push(frame);

                    // this is done, so don't keep it
                    false
                }
                Poll::Ready(Err(())) => {
                    // this state became invalid, so don't keep it.
                    false
                }
            }
        });
    }
}

/* todo: fix emitter #27
impl Scanner<Complex<f32>> for Demodulator {
    type Output = Option<RawFrame>;

    #[inline]
    fn scan(&mut self, sample: Complex<f32>) -> Self::Output {
        self.push(sample.norm())
    }
}

impl Scanner<f32> for Demodulator {
    type Output = Option<RawFrame>;

    #[inline]
    fn scan(&mut self, sample: f32) -> Self::Output {
        self.push(sample)
    }
} */

#[derive(Clone, Debug)]
struct State {
    sample_num: usize,
    frame_buffer: [u8; LENGTH_LONG],
    num_samples: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            sample_num: 1,
            frame_buffer: [0; _],
            num_samples: DATA_SAMPLES_SHORT + PREAMBLE_SAMPLES,
        }
    }
}

impl State {
    fn push(&mut self, edge: Edge) -> Poll<Result<RawFrame, ()>> {
        assert!(self.sample_num < self.num_samples);

        if self.sample_num < PREAMBLE_SAMPLES {
            // we're still in the preamble

            if let Some(expected_edge) = PREAMBLE_EDGES[self.sample_num] {
                if edge == expected_edge {
                    // still matching
                }
                else {
                    // current edge doesn't match preamble, abort
                    return Poll::Ready(Err(()));
                }
            }
        }
        else {
            // data part

            // only with the second sample per bit we can actually figure out
            // the bit
            if self.sample_num % 2 == 1 {
                if edge == Edge::Falling {
                    // falling edge is a 1, and only those we need to set

                    let bit = (self.sample_num - PREAMBLE_SAMPLES) / 2;

                    if bit == 0 {
                        // this decides if this is a short ot long frame. 1 for
                        // long frames.
                        self.num_samples = DATA_SAMPLES_LONG + PREAMBLE_SAMPLES;
                    }

                    let byte = bit / 8;
                    let bit = bit % 8;
                    // note: first bit received corresponds to MSB
                    self.frame_buffer[byte] |= 0x80 >> bit;
                }
                else {
                    // rising edge is a 0. we don't need to write this.
                    //
                    // if the first bit is 0, this indicates that this is a
                    // short frame, but we initialize it to that anyway.
                }
            }
        }

        // update state
        self.sample_num += 1;

        if self.sample_num == self.num_samples {
            // we have the full frame. return it.

            let frame = if self.num_samples == DATA_SAMPLES_SHORT + PREAMBLE_SAMPLES {
                let mut data = [0; _];
                data.copy_from_slice(&self.frame_buffer[0..LENGTH_SHORT]);
                RawFrame::ModeSShort { data }
            }
            else {
                RawFrame::ModeSLong {
                    data: self.frame_buffer,
                }
            };

            Poll::Ready(Ok(frame))
        }
        else {
            // not done yet, continue
            Poll::Pending
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Edge {
    Rising,
    Falling,
}

impl Edge {
    fn from_samples(samples: [f32; 2]) -> Self {
        // note: not 100% symmetrical as we need a tie-breaker or deal with the
        // rare case that both are equal
        if samples[0] < samples[1] {
            Self::Rising
        }
        else {
            Self::Falling
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Demodulator,
        RawFrame,
    };

    fn modulate(data: &[u8], mut sample: impl FnMut(bool) -> f32) -> Vec<f32> {
        let mut samples = vec![];

        // 0, 2, 7, 9 are high
        let mut preamble: u16 = 0b1010_0001_0100_0000;
        for _ in 0..16 {
            if preamble & 0x8000 == 0 {
                samples.push(sample(false));
            }
            else {
                samples.push(sample(true));
            }
            preamble <<= 1;
        }

        for mut byte in data.iter().copied() {
            for _ in 0..8 {
                if byte & 0x80 == 0 {
                    // bit=0 raising edge
                    samples.push(sample(false));
                    samples.push(sample(true));
                }
                else {
                    // bit=1 falling edge
                    samples.push(sample(true));
                    samples.push(sample(false));
                }
                byte <<= 1;
            }
        }

        samples
    }

    fn signal(signal: bool) -> f32 {
        if signal { 1.0 } else { 0.0 }
    }

    fn demod(mut demodulator: Demodulator, samples: &[f32], check: impl Fn(&RawFrame) -> bool) {
        let mut output = vec![];

        for sample in samples {
            demodulator.push(*sample, &mut output);
        }

        if !output.into_iter().any(|frame| {
            dbg!(&frame);
            check(&frame)
        }) {
            panic!("None of the matched frames is the correct one.")
        };
    }

    #[test]
    fn it_demodulates_a_frame() {
        let input = b"\x8d\x40\x74\xb5\x23\x15\xa6\x76\xdd\x13\xa0\x66\x29\x67";

        let samples = modulate(input, signal);

        let demodulator = Demodulator::default();
        demod(demodulator, &samples, |frame| {
            match frame {
                RawFrame::ModeSLong { data } => data == input,
                _ => false,
            }
        });
    }
}
