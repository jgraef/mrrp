use std::{
    hint::black_box,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::FutureExt;
use mrrp::{
    buf::SampleBufMut,
    filter::design::{
        FilterDesign,
        Lowpass,
        pm_remez::pm_remez,
    },
    io::{
        AsyncReadSamples,
        AsyncReadSamplesExt,
        Cursor,
        FiniteStream,
        ReadBuf,
        Remaining,
        StreamLength,
    },
    source::white_noise,
};
use num_complex::Complex;
use pin_project_lite::pin_project;

#[test]
fn bench_fir_single_sample_buffered_bug() {
    // Bug showed while benchmarking:
    //
    // Benchmarking buffering/fir single-sample buffered: Warming up for 3.0000 s
    // thread 'main' panicked at
    // /home/emma/code/mrrp/mrrp/src/buf/uninit_slice.rs:105:9: assertion `left ==
    // right` failed   left: 1  right: 16383
    //
    // extracted from benches/buffering.rs
    //
    // fixed by: when copying from the internal buffer to the destination buffer,
    // the length of the destination buffer slice was not limited to the amount
    // available.

    let num_samples = 0x100000;

    let mut samples = vec![];
    white_noise::<Complex<f32>>()
        .limit(num_samples)
        .read_to_end(&mut samples)
        .now_or_never()
        .expect("white noise returned pending")
        .expect("white noise returned error");
    let input = Cursor::new(&samples[..]);

    let filter = pm_remez(Lowpass::new(0.25, 0.01, 0.05, 0.05), 17)
        .unwrap()
        .fir_filter();

    let filtered_single_samples =
        black_box(SingleSampleStream::new(input.clone())).scan_in_place_with(filter);

    read_stream(filtered_single_samples.clone().buffered(0x4000));
}

fn read_stream<R, S>(mut stream: R)
where
    R: AsyncReadSamples<S> + Unpin + FiniteStream,
{
    let mut output = vec![];
    stream
        .read_to_end(&mut output)
        .now_or_never()
        .expect("read_to_end returned pending")
        .expect("read_to_end returned an error");
    let _ = black_box(output);
}

pin_project! {
    #[derive(Clone, Copy, Debug)]
    struct SingleSampleStream<R> {
        #[pin]
        inner: R,
    }
}

impl<R> SingleSampleStream<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }
}

impl<R, S> AsyncReadSamples<S> for SingleSampleStream<R>
where
    R: AsyncReadSamples<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        if buffer.has_remaining_mut() {
            let mut read_buf = buffer.take(1);
            match self.project().inner.poll_read_samples(cx, &mut read_buf) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Ready(Ok(())) => {
                    let filled = read_buf.filled().len();
                    let initialized = read_buf.initialized().len();

                    assert!(filled <= 1);
                    if filled == 1 {
                        unsafe {
                            buffer.assume_init(initialized);
                        }
                        buffer.set_filled(buffer.filled().len() + filled);
                    }
                }
            }
        }

        Poll::Ready(Ok(()))
    }
}

impl<R> StreamLength for SingleSampleStream<R>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> Remaining {
        self.inner.remaining()
    }
}

impl<R> FiniteStream for SingleSampleStream<R> where R: FiniteStream {}
