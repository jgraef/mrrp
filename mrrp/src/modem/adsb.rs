//! 1090 MHz Mode-S demodulator
//!
//! <https://www.radartutorial.eu/13.ssr/sr24.en.html>
//! <https://www.idc-online.com/technical_references/pdfs/electronic_engineering/Mode_S_Reply_Encoding.pdf>
#![allow(dead_code)]

use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::Stream;
use num_complex::Complex;
use pin_project_lite::pin_project;

use crate::io::{
    AsyncReadSamples,
    AsyncReadSamplesExt,
    MapInPlacePod,
    ReadBuf,
};

/// Preamble: 8 µs / 16 samples
const PREAMBLE_SAMPLES: usize = 16;

/// Sample rate: 2 samples/µs
pub const SAMPLE_RATE: u32 = 2_000_000;

/// Mode S downlink frequency: 1090 MHz
pub const DOWNLINK_FREQUENCY: u32 = 1_090_000_000;

/// Mode S uplink frequency: 1030 MHz
pub const UPLINK_FREQUENCY: u32 = 1_030_000_000;

#[derive(Clone, Copy, Debug)]
pub enum Frame {
    ModeAc { data: [u8; 2] },
    ModeSShort { data: [u8; 7] },
    ModeSLong { data: [u8; 14] },
}

impl AsRef<[u8]> for Frame {
    fn as_ref(&self) -> &[u8] {
        match self {
            Frame::ModeAc { data } => &data[..],
            Frame::ModeSShort { data } => &data[..],
            Frame::ModeSLong { data } => &data[..],
        }
    }
}

enum DemodFail {
    NotEnoughSamples,
    Invalid,
}

#[derive(Debug)]
pub struct Demodulator {
    quality: Quality,
    num_errors: usize,
    max_errors: usize,
}

impl Default for Demodulator {
    fn default() -> Self {
        Self::new(Default::default(), 5)
    }
}

impl Demodulator {
    pub fn new(quality: Quality, max_errors: usize) -> Self {
        Self {
            quality,
            num_errors: 0,
            max_errors,
        }
    }

    pub fn next(&mut self, cursor: &mut Cursor<f32>) -> Option<Frame> {
        while find_preamble(cursor) {
            //tracing::debug!(?cursor.position, "found preamble");

            let mut frame_cursor = *cursor;

            match self.read_frame(&mut frame_cursor) {
                Ok(frame) => {
                    // found a frame!
                    // set main cursor to the position of the frame cursor
                    cursor.position = frame_cursor.position;
                    return Some(frame);
                }
                Err(DemodFail::NotEnoughSamples) => {
                    // cursor position should remain at start of preamble
                    return None;
                }
                Err(DemodFail::Invalid) => {
                    // find next preamble starting from end of previous preamble
                    // so the main cursor stays unchanged and we do nothing here
                }
            }
        }

        None
    }

    fn read_frame(&mut self, cursor: &mut Cursor<f32>) -> Result<Frame, DemodFail> {
        self.num_errors = 0;

        let first_byte = self.read_byte(cursor)?;

        if first_byte & 0x80 == 0 {
            Ok(Frame::ModeSShort {
                data: self.read_frame_rest(first_byte, cursor)?,
            })
        }
        else {
            Ok(Frame::ModeSLong {
                data: self.read_frame_rest(first_byte, cursor)?,
            })
        }
    }

    fn read_frame_rest<const N: usize>(
        &mut self,
        first_byte: u8,
        cursor: &mut Cursor<f32>,
    ) -> Result<[u8; N], DemodFail> {
        let mut data = [0u8; N];
        data[0] = first_byte;
        for i in 1..N {
            data[i] = self.read_byte(cursor)?;
        }
        Ok(data)
    }

    fn read_bit(&self, cursor: &mut Cursor<f32>) -> Result<bool, bool> {
        // these should exist, since we read a preamble first
        let a = cursor.samples[cursor.position - 2];
        let b = cursor.samples[cursor.position - 1];

        let c = cursor.samples[cursor.position];
        let d = cursor.samples[cursor.position + 1];

        cursor.advance(2);

        let bit_p = a > b;
        let bit = c > d;

        // todo: this could be implemented with a few bitmask really

        match self.quality {
            Quality::NoChecks => Ok(bit),
            Quality::HalfBit => {
                if bit && bit_p && b > c {
                    Err(bit)
                }
                else if !bit && !bit_p && b < c {
                    Err(bit)
                }
                else {
                    Ok(bit)
                }
            }
            Quality::OneBit => {
                if bit && bit_p && c > b {
                    Ok(true)
                }
                else if bit && !bit_p && d < b {
                    Ok(true)
                }
                else if !bit && bit_p && d > b {
                    Ok(false)
                }
                else if !bit && !bit_p && c < b {
                    Ok(false)
                }
                else {
                    Err(bit)
                }
            }
            Quality::TwoBits => {
                if bit && bit_p && c > b && d < a {
                    Ok(true)
                }
                else if bit && !bit_p && c > a && d < b {
                    Ok(true)
                }
                else if !bit && bit_p && c < a && d > b {
                    Ok(false)
                }
                else if !bit && !bit_p && c < b && d > a {
                    Ok(false)
                }
                else {
                    Err(bit)
                }
            }
        }
    }

    fn read_byte(&mut self, cursor: &mut Cursor<f32>) -> Result<u8, DemodFail> {
        let mut byte = 0;

        if cursor.remaining().len() < 2 * 8 {
            Err(DemodFail::NotEnoughSamples)
        }
        else {
            for _ in 0..8 {
                byte <<= 1;
                let bit = self.read_bit(cursor).or_else(|bit| {
                    self.num_errors += 1;
                    if self.num_errors <= self.max_errors {
                        Ok(bit)
                    }
                    else {
                        // rtl_adsb.c does change the previous bits, but I don't get how that works.
                        // Wouldn't that break the next bit reads?
                        //
                        // <https://github.com/rtlsdrblog/rtl-sdr-blog/blob/240bd0e1e6d9f64361b6949047468958cd08aa31/src/rtl_adsb.c#L300>
                        Err(DemodFail::Invalid)
                    }
                })?;

                if bit {
                    byte |= 1;
                }
            }

            Ok(byte)
        }
    }
}

fn is_preamble(samples: &[f32]) -> bool {
    let mut low = f32::MIN;
    let mut high = f32::MAX;

    for i in 0..PREAMBLE_SAMPLES {
        match i {
            0 | 2 | 7 | 9 => {
                high = samples[i];
            }
            _ => {
                low = samples[i];
            }
        }

        if high <= low {
            return false;
        }
    }

    true
}

fn find_preamble(cursor: &mut Cursor<f32>) -> bool {
    loop {
        let remaining = cursor.remaining();
        if remaining.len() >= PREAMBLE_SAMPLES {
            if is_preamble(remaining) {
                cursor.advance(PREAMBLE_SAMPLES);
                break true;
            }
            cursor.advance(1);
        }
        else {
            break false;
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quality {
    NoChecks,
    HalfBit,
    #[default]
    OneBit,
    TwoBits,
}

pin_project! {
    #[derive(Debug)]
    pub struct DemodulateStream<T> {
        #[pin]
        stream: MapInPlacePod<T, Complex<f32>, fn(Complex<f32>) -> f32>,
        demodulator: Demodulator,
        buffer: Vec<f32>,
        read_pos: usize,
        write_pos: usize,
        num_samples: usize,
    }
}

impl<T: AsyncReadSamples<Complex<f32>>> DemodulateStream<T> {
    pub fn new(stream: T, demodulator: Demodulator, buffer_size: usize) -> Self {
        Self {
            stream: stream.map_in_place_pod(|sample| sample.re * sample.re + sample.im * sample.im),
            demodulator,
            buffer: vec![0.0; buffer_size],
            read_pos: 0,
            write_pos: 0,
            num_samples: 0,
        }
    }
}

impl<T: AsyncReadSamples<Complex<f32>>> Stream for DemodulateStream<T> {
    type Item = Result<Frame, T::Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            let this = self.as_mut().project();

            /*tracing::debug!(
                read_pos = *this.read_pos,
                write_pos = *this.write_pos,
                num_samples = *this.num_samples
            );*/

            if *this.read_pos < *this.num_samples {
                let mut cursor = Cursor {
                    samples: &this.buffer[..*this.num_samples],
                    position: *this.read_pos,
                };

                if let Some(frame) = this.demodulator.next(&mut cursor) {
                    *this.read_pos = cursor.position;
                    return Poll::Ready(Some(Ok(frame)));
                }
                else {
                    let position = cursor.position;
                    this.buffer.copy_within(position..*this.num_samples, 0);
                    *this.write_pos = *this.num_samples - position;
                    *this.read_pos = 0;
                    *this.num_samples = 0;
                }
            }
            else {
                let mut read_buf = ReadBuf::new(&mut this.buffer[*this.write_pos..]);
                match this.stream.poll_read_samples(cx, &mut read_buf) {
                    Poll::Pending => return Poll::Pending,
                    Poll::Ready(Err(error)) => return Poll::Ready(Some(Err(error))),
                    Poll::Ready(Ok(())) => {
                        if read_buf.filled().is_empty() {
                            return Poll::Ready(None);
                        }

                        *this.num_samples = *this.write_pos + read_buf.filled().len();
                        *this.read_pos = 0;
                        *this.write_pos = 0;
                    }
                }
            }
        }
    }
}

// todo: use SampleBuf instead, buf we need examine past samples sometimes.

#[derive(Clone, Copy, Debug)]
pub struct Cursor<'a, S> {
    pub samples: &'a [S],
    pub position: usize,
}

impl<'a, S> Cursor<'a, S> {
    #[inline(always)]
    pub fn advance(&mut self, amount: usize) {
        self.position += amount;
    }

    #[inline(always)]
    pub fn remaining(&self) -> &[S] {
        &self.samples[self.position..]
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Demodulator,
        Frame,
        Quality,
    };
    use crate::modem::adsb::Cursor;

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

    #[test]
    fn it_demodulates_a_frame() {
        let input = b"\x8d\x40\x74\xb5\x23\x15\xa6\x76\xdd\x13\xa0\x66\x29\x67";

        let samples = modulate(input, signal);

        let mut demodulator = Demodulator::new(Quality::NoChecks, 0);
        let mut cursor = Cursor {
            samples: &samples[..],
            position: 0,
        };

        let frame = demodulator.next(&mut cursor).expect("no frame demodulated");
        match frame {
            Frame::ModeSLong { data } => {
                assert_eq!(&data, input);
            }
            _ => panic!("unexpected frame: {:?}", frame),
        }
    }
}
