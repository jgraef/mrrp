//!
//! # TODO
//!
//! CIC filter: <https://www.researchgate.net/publication/228648728_Understanding_cascaded_integrator-comb_filters>

use std::{
    ops::{
        AddAssign,
        Div,
        Mul,
    },
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use num_traits::{
    FromPrimitive,
    Zero,
};

use crate::{
    GetSampleRate,
    buf::{
        SampleBufMut,
        SamplesMut,
    },
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        ReadBuf,
    },
    sample::Sample,
};

#[derive(Clone, Debug)]
pub struct AverageDecimate<T, S> {
    input: T,
    decimate: usize,
    accumulator: Option<(S, usize)>,
}

impl<T, S> AverageDecimate<T, S> {
    #[inline]
    pub fn new(input: T, decimate: usize) -> Self {
        Self {
            input,
            decimate,
            accumulator: None,
        }
    }
}

impl<T, S> GetSampleRate for AverageDecimate<T, S>
where
    T: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.input.sample_rate() / self.decimate as f32
    }
}

impl<T, S> AsyncReadSamples<S> for AverageDecimate<T, S>
where
    T: AsyncReadSamples<S> + Unpin,
    S: Sample + Clone + Zero + AddAssign + Div<Output = S> + Unpin + Div<S::Scalar, Output = S>,
    S::Scalar: FromPrimitive,
{
    type Error = T::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = &mut *self;

        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        loop {
            match this.input.poll_read_samples_unpin(cx, buffer) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let accumulator = this.accumulator.get_or_insert_with(|| (S::zero(), 0));
                    let mut write_pos = 0;

                    for read_pos in 0..buffer.filled().len() {
                        accumulator.0 += buffer.filled()[read_pos].clone();
                        accumulator.1 += 1;
                        if accumulator.1 == this.decimate {
                            let average = std::mem::replace(&mut accumulator.0, S::zero())
                                / <S::Scalar>::from_usize(this.decimate).unwrap();
                            buffer.filled_mut()[write_pos] = average;
                            write_pos += 1;
                            accumulator.1 = 0;
                        }
                    }

                    buffer.set_filled(write_pos);

                    if write_pos > 0 {
                        return Poll::Ready(Ok(()));
                    }
                }
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FirFilter<T, S, C> {
    pub input: T,
    pub coefficients: Vec<C>,
    pub delayed: SamplesMut<S>,
}

impl<T, S, C> AsyncReadSamples<S> for FirFilter<T, S, C>
where
    T: AsyncReadSamples<S> + Unpin,
    S: Clone + Zero + AddAssign + Unpin,
    C: Unpin,
    for<'a> &'a C: Mul<&'a S, Output = S>,
{
    type Error = T::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = &mut *self;

        match this.input.poll_read_samples_unpin(cx, buffer) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let read = buffer.filled_mut();
                convolve_delayed(&this.coefficients, &mut this.delayed, read);
                todo!();
            }
        }
    }
}

fn convolve_delayed<S, C>(coeffients: &[C], delayed: &mut SamplesMut<S>, read: &mut [S]) -> usize
where
    S: Clone + Zero + AddAssign,
    for<'a> &'a C: Mul<&'a S, Output = S>,
{
    assert!(delayed.len() < coeffients.len());

    // if we're missing samples in the delay buffer, we need to copy them over,
    // because the read buffer will be overwritten.
    let missing_in_delay = coeffients.len().saturating_sub(delayed.len() + 1);
    delayed.put(&read[..missing_in_delay]);

    let read_start = missing_in_delay;

    let mut i = 0;

    while i < delayed.len() {
        let mut s = S::zero();
        let mut c = coeffients.len() - 1;

        for j in i..delayed.len() {
            s += &coeffients[c] * &delayed[j];
            c -= 1;
        }

        for j in 0..=i {
            s += &coeffients[c] * &read[read_start + j];
            c -= 1;
        }

        read[i] = s;

        i += 1;
    }

    let mut i0 = 0;
    let n = read.len() - missing_in_delay;
    assert_eq!(i - i0 + 1, coeffients.len());

    while i < n {
        let mut s = S::zero();
        let mut c = coeffients.len() - 1;

        for j in i0..=i {
            s += &coeffients[c] * &read[read_start + j];
            c -= 1;
        }

        read[i] = s;

        i += 1;
        i0 += 1;
    }

    n
}
