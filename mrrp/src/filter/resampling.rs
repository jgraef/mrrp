use std::{
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use num_traits::Zero;
use pin_project_lite::pin_project;

use crate::{
    GetSampleRate,
    buf::SampleBufMut,
    io::{
        AsyncReadSamples,
        ReadBuf,
    },
};

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

impl<R, S> AsyncReadSamples<S> for Decimate<R>
where
    R: AsyncReadSamples<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
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

pin_project! {
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

impl<R, S> AsyncReadSamples<S> for Interpolate<R>
where
    R: AsyncReadSamples<S>,
    S: Zero,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
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
                    let mut sample = S::zero();

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
