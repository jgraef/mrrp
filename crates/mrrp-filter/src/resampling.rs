use std::{
    ops::{
        AddAssign,
        Div,
    },
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::{
    buf::SampleBufMut,
    sample::Sample,
    signal::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
        Remaining,
        StreamLength,
    },
};
use num_traits::{
    FromPrimitive,
    Zero,
};
use pin_project_lite::pin_project;

pin_project! {
    #[derive(Clone, Debug)]
    pub struct Decimate<R> {
        #[pin]
        input: R,
        factor: usize,
        counter: usize,
    }
}

impl<R> Decimate<R> {
    pub fn new(input: R, factor: usize) -> Self {
        assert!(factor != 0);
        Self {
            input,
            factor,
            counter: 0,
        }
    }
}

impl<R> AsyncReadSamples for Decimate<R>
where
    R: AsyncReadSamples,
{
    type Sample = R::Sample;
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        let mut read_buf = ReadBuf::uninit(buffer.unfilled_mut());

        match this.input.poll_read_samples(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let mut read_pos = 0;
                let mut write_pos = 0;

                let num_samples = read_buf.filled().len();
                let buffer_uninit = read_buf.inner_mut();

                while read_pos < num_samples {
                    // note: we drop all the samples that are not written back

                    let sample = unsafe { buffer_uninit[read_pos].assume_init_read() };
                    read_pos += 1;

                    if *this.counter == 0 {
                        buffer_uninit.write_sample(write_pos, sample);
                        write_pos += 1;
                    }

                    *this.counter += 1;
                    if this.counter == this.factor {
                        *this.counter = 0;
                    }
                }

                unsafe {
                    buffer.assume_init(write_pos);
                }
                buffer.set_filled(buffer.filled().len() + write_pos);

                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<R> GetSampleRate for Decimate<R>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.input.sample_rate() / self.factor as f32
    }
}

impl<R> StreamLength for Decimate<R>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.input
            .remaining()
            .map(|num_samples| (num_samples + self.counter) / self.factor)
    }
}

pin_project! {
    /// Interpolated stream
    ///
    /// This is created with the [`interpolate`](AsyncReadSamplesFilterExt::interpolate) method.
    ///
    /// # TODO
    ///
    /// We think it doesn't matter with what value you pad. So we could accept this as an argument.
    /// Then we don't need the [`Sample`] trait bound for the equilibrium value.
    #[derive(Clone, Debug)]
    pub struct Interpolate<R> {
        #[pin]
        input: R,
        factor: usize,
        counter: usize,
    }
}

impl<R> Interpolate<R> {
    pub fn new(input: R, factor: usize) -> Self {
        assert!(factor != 0);
        Self {
            input,
            factor,
            counter: 0,
        }
    }
}

impl<R> AsyncReadSamples for Interpolate<R>
where
    R: AsyncReadSamples,
    R::Sample: Sample,
{
    type Sample = R::Sample;
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        if !buffer.has_remaining_mut() {
            return Poll::Ready(Ok(()));
        }

        // the number of samples we can read so that after interpolation we have no read
        // samples left.
        //let num_samples_read = (buffer.remaining() -
        // *this.counter).div_ceil(*this.factor);
        let num_samples_read = buffer.remaining() / *this.factor;
        assert!(num_samples_read != 0);

        // read to the very end of the buffer so we can interpolate from the start
        let buffer_unfilled = buffer.unfilled_mut();
        let read_start_pos = buffer_unfilled.len() - num_samples_read;
        let mut read_buf = ReadBuf::uninit(&mut buffer_unfilled[read_start_pos..]);

        match this.input.poll_read_samples(cx, &mut read_buf) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) => {
                let mut read_pos = read_start_pos;
                let read_end_pos = read_start_pos + read_buf.filled().len();
                let mut write_pos = 0;

                while read_pos < read_end_pos {
                    let mut sample = <R::Sample>::EQUILIBRIUM;

                    if *this.counter == 0 {
                        sample = unsafe { buffer_unfilled[read_pos].assume_init_read() };
                        read_pos += 1;
                    }

                    *this.counter += 1;
                    if *this.counter == *this.factor {
                        *this.counter = 0;
                    }

                    assert!(write_pos < read_pos, "will overwrite future read positions");
                    buffer_unfilled.write_sample(write_pos, sample);
                    write_pos += 1;
                }

                assert_eq!(read_pos, read_end_pos, "didnt read all samples");
                unsafe {
                    buffer.assume_init(write_pos);
                }
                buffer.set_filled(buffer.filled().len() + write_pos);

                Poll::Ready(Ok(()))
            }
        }
    }
}

impl<R> GetSampleRate for Interpolate<R>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.input.sample_rate() * self.factor as f32
    }
}

impl<R> StreamLength for Interpolate<R>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.input
            .remaining()
            .map(|num_samples| num_samples * self.factor - self.counter)
    }
}

/// A simple averaging low-pass filter for decimation.
///
/// This takes the average of N samples to produce one sample. It is equivalent
/// to a FIR filter with N coefficients of 1/N followed by a decimation of N
/// samples to 1.
///
/// <https://en.wikipedia.org/wiki/Finite_impulse_response#Moving_average_example>
#[derive(Clone, Debug)]
pub struct AverageDecimate<T>
where
    T: AsyncReadSamples,
{
    input: T,
    decimate: usize,
    accumulator: (T::Sample, usize),
}

impl<T> AverageDecimate<T>
where
    T: AsyncReadSamples,
    T::Sample: Zero,
{
    #[inline]
    pub fn new(input: T, decimate: usize) -> Self {
        Self {
            input,
            decimate,
            accumulator: (<T::Sample>::zero(), 0),
        }
    }
}

impl<T> GetSampleRate for AverageDecimate<T>
where
    T: AsyncReadSamples + GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.input.sample_rate() / self.decimate as f32
    }
}

impl<T> StreamLength for AverageDecimate<T>
where
    T: AsyncReadSamples + StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.input
            .remaining()
            .map(|num_samples| (num_samples + self.accumulator.1) / self.decimate)
    }
}

impl<T> AsyncReadSamples for AverageDecimate<T>
where
    T: AsyncReadSamples + Unpin,
    T::Sample: Sample
        + Clone
        + Zero
        + AddAssign
        + Div<Output = T::Sample>
        + Unpin
        + Div<<T::Sample as Sample>::Scalar, Output = T::Sample>,
    <T::Sample as Sample>::Scalar: FromPrimitive,
{
    type Sample = T::Sample;
    type Error = T::Error;

    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = &mut *self;

        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        loop {
            let filled_before = buffer.filled().len();

            match Pin::new(&mut this.input).poll_read_samples(cx, buffer) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    // note: we start overwriting from the beginning of the **newly** filled buffer,
                    // since that's what we just read. even though this runs in
                    // a loop, we exit the loop as soon as we have filled the
                    // buffer with any number of decimated samples.

                    let mut write_pos = filled_before;
                    let filled = buffer.filled().len();

                    if filled == filled_before {
                        // nothing was read, so this is an eof
                        return Poll::Ready(Ok(()));
                    }

                    for read_pos in filled_before..filled {
                        this.accumulator.0 += buffer.filled()[read_pos].clone();
                        this.accumulator.1 += 1;
                        if this.accumulator.1 == this.decimate {
                            let average =
                                std::mem::replace(
                                    &mut this.accumulator.0,
                                    <T::Sample as Zero>::zero(),
                                ) / <<T::Sample as Sample>::Scalar>::from_usize(this.decimate)
                                    .unwrap();
                            buffer.filled_mut()[write_pos] = average;
                            write_pos += 1;
                            this.accumulator.1 = 0;
                        }
                    }

                    buffer.set_filled(write_pos);

                    if write_pos > 0 {
                        // we produced at least one decimated sample, so we can return
                        return Poll::Ready(Ok(()));
                    }
                }
            }
        }
    }
}
