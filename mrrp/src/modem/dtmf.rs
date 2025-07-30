use std::{
    fmt::{
        Debug,
        Display,
    },
    pin::Pin,
    task::{
        Context,
        Poll,
        ready,
    },
};

use futures_util::Stream;
use num_complex::Complex;
use pin_project_lite::pin_project;

use crate::{
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        GetSampleRate,
        ReadBuf,
        Remaining,
        SizeHint,
        StreamLength,
        combinators::Limited,
    },
    source::{
        ComplexSinusoid,
        SignalGenerator,
        SignalGeneratorReadSamples,
    },
};

pin_project! {
    #[derive(Clone, Copy, Debug)]
    pub struct DtmfEncoder<S> {
        #[pin]
        symbols: S,
        num_samples_per_tone: usize,
        sample_rate: f32,
        current_tone: Option<Limited<SignalGeneratorReadSamples<DtmfTone>>>,
    }
}

impl<S> DtmfEncoder<S> {
    pub fn new(symbols: S, sample_rate: f32, tone_duration: f32) -> Self {
        let num_samples_per_tone = (tone_duration * sample_rate).round() as usize;
        Self {
            symbols,
            num_samples_per_tone,
            sample_rate,
            current_tone: None,
        }
    }
}

impl<S, E> AsyncReadSamples<Complex<f32>> for DtmfEncoder<S>
where
    S: Stream<Item = Result<DtmfSymbol, E>>,
    E: std::error::Error,
{
    type Error = E;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Complex<f32>>,
    ) -> Poll<Result<(), Self::Error>> {
        loop {
            let this = self.as_mut().project();

            if let Some(current_tone) = this.current_tone {
                //dbg!(&current_tone);

                let filled_before = buffer.filled().len();
                //dbg!(filled_before);

                ready!(Pin::new(current_tone).poll_read_samples(cx, buffer))
                    .unwrap_or_else(|e| match e {});

                //dbg!(buffer.filled().len());

                if buffer.filled().len() == filled_before {
                    *this.current_tone = None;
                }
                else {
                    break;
                }
            }
            else if let Some(symbol) = ready!(this.symbols.poll_next(cx)).transpose()? {
                tracing::debug!(?symbol);
                *this.current_tone = Some(
                    SignalGeneratorReadSamples::new(symbol.tone(*this.sample_rate))
                        .limit(*this.num_samples_per_tone),
                );
            }
            else {
                break;
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl<S> GetSampleRate for DtmfEncoder<S> {
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.sample_rate
    }
}

impl<S> StreamLength for DtmfEncoder<S>
where
    S: Stream,
{
    fn remaining(&self) -> Remaining {
        let (lower_bound, upper_bound) = self.symbols.size_hint();
        upper_bound
            .filter(|upper_bound| *upper_bound == lower_bound)
            .map_or(Remaining::Unknown, |num_symbols| {
                let current_tone_remaining = self
                    .current_tone
                    .as_ref()
                    .map_or(0, |current_tone| current_tone.len());
                Remaining::Finite {
                    num_samples: num_symbols * self.num_samples_per_tone + current_tone_remaining,
                }
            })
    }

    fn size_hint(&self) -> SizeHint {
        let (lower_bound, upper_bound) = self.symbols.size_hint();
        let current_tone_remaining = self
            .current_tone
            .as_ref()
            .map_or(0, |current_tone| current_tone.len());
        SizeHint {
            lower_bound: lower_bound * self.num_samples_per_tone + current_tone_remaining,
            upper_bound: upper_bound.map(|upper_bound| {
                upper_bound * self.num_samples_per_tone + current_tone_remaining
            }),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct DtmfSymbol(u8);

impl DtmfSymbol {
    #[inline]
    pub fn tone(&self, sample_rate: f32) -> DtmfTone {
        let [row, column] = self.row_column_frequency();
        DtmfTone::new(row, column, sample_rate)
    }

    pub fn row_column(&self) -> [usize; 2] {
        match self.0 {
            b'1' => [0, 0],
            b'2' => [0, 1],
            b'3' => [0, 2],
            b'A' => [0, 3],
            b'4' => [1, 0],
            b'5' => [1, 1],
            b'6' => [1, 2],
            b'B' => [1, 3],
            b'7' => [2, 0],
            b'8' => [2, 1],
            b'9' => [2, 2],
            b'C' => [2, 3],
            b'*' => [3, 0],
            b'0' => [3, 1],
            b'#' => [3, 2],
            b'D' => [3, 3],
            _ => unreachable!(),
        }
    }

    pub fn row_column_frequency(&self) -> [f32; 2] {
        let [row, column] = self.row_column();
        [TONES_ROWS[row], TONES_COLUMNS[column]]
    }
}

impl Debug for DtmfSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        <char as Debug>::fmt(&char::from(*self), f)
    }
}

impl Display for DtmfSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", char::from(*self))
    }
}

impl TryFrom<char> for DtmfSymbol {
    type Error = char;

    fn try_from(value: char) -> Result<Self, Self::Error> {
        let value = value.to_ascii_uppercase();
        if value.is_ascii_digit() || matches!(value, 'A'..='D') || value == '*' || value == '#' {
            Ok(Self(u8::try_from(value).unwrap()))
        }
        else {
            Err(value)
        }
    }
}

impl From<DtmfSymbol> for char {
    fn from(value: DtmfSymbol) -> Self {
        char::from_u32(value.0.into()).unwrap()
    }
}

pub const TONES_ROWS: [f32; 4] = [697.0, 770.0, 852.0, 941.0];
pub const TONES_COLUMNS: [f32; 4] = [1209.0, 1336.0, 1477.0, 1633.0];

#[derive(Clone, Copy, Debug)]
pub struct DtmfTone {
    row: ComplexSinusoid,
    column: ComplexSinusoid,
}

impl DtmfTone {
    pub fn new(row_frequency: f32, column_frequency: f32, sample_rate: f32) -> Self {
        Self {
            row: ComplexSinusoid::new(row_frequency, sample_rate),
            column: ComplexSinusoid::new(column_frequency, sample_rate),
        }
    }
}

impl SignalGenerator for DtmfTone {
    type Sample = Complex<f32>;

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.row.set_sample_rate(sample_rate);
        self.column.set_sample_rate(sample_rate);
    }

    fn next(&mut self) -> Self::Sample {
        self.row.next() + self.column.next()
    }
}

impl GetSampleRate for DtmfTone {
    fn sample_rate(&self) -> f32 {
        self.row.sample_rate()
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use futures_util::{
        FutureExt,
        stream,
    };

    use crate::{
        io::{
            AsyncReadSamples,
            AsyncReadSamplesExt,
            EofError,
            StreamLength,
        },
        modem::dtmf::{
            DtmfEncoder,
            DtmfSymbol,
        },
    };

    #[test]
    fn bug_dtmf_not_playing_sounds() {
        // this bug manifested as it not playing any sounds when unbuffered, or playing
        // only a short sound when buffered. the reason for the bug was a
        // missing break in the read loop when a successful read was done. it would then
        // proceed to call poll_read again, which would return 0 because the buffer was
        // already full. it would do this multiple times, cycling through the tones,
        // until no tone was left, and return.

        let sample_rate = 44100.0;
        let tone_duration = 1.0;

        // parse symbols
        let symbols = "017612345"
            .chars()
            .map(|c| DtmfSymbol::try_from(c))
            .collect::<Result<Vec<_>, _>>()
            .unwrap_or_else(|c| panic!("Invalid DTMF symbol: {c}"));

        // convert to stream
        let symbols = stream::iter(
            symbols
                .into_iter()
                .map(|symbol| Ok::<_, Infallible>(symbol)),
        );

        // encode
        let encoded = DtmfEncoder::new(symbols, sample_rate, tone_duration);

        let stream = encoded.map(|sample| sample.re);

        let expected_num_samples = stream.remaining().finite_length().unwrap();

        assert_eq!(expected_num_samples, count_stream(stream.clone()));
        assert_eq!(expected_num_samples, count_stream(stream.buffered(0x4000)));
    }

    fn count_stream<R>(mut stream: R) -> usize
    where
        R: AsyncReadSamples<f32> + Unpin,
    {
        let mut num_samples = 0;
        loop {
            match stream.read_sample().now_or_never().unwrap() {
                Ok(_sample) => {
                    num_samples += 1;
                }
                Err(EofError::Eof { .. }) => break,
                Err(EofError::Other(error)) => panic!("{error}"),
            }
        }
        num_samples
    }
}
