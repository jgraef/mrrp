use std::{
    hint::black_box,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use criterion::{
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
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
        EofError,
        ReadBuf,
        Remaining,
        StreamLength,
    },
    source::white_noise,
};
use num_complex::Complex;
use pin_project_lite::pin_project;

pub fn bench_buffering(c: &mut Criterion) {
    let num_samples = 0x100000;

    let mut group = c.benchmark_group("buffering");
    group.throughput(Throughput::Elements(num_samples as u64));

    let mut samples = vec![];
    white_noise::<Complex<f32>>()
        .limit(num_samples)
        .read_to_end(&mut samples)
        .now_or_never()
        .expect("white noise returned pending")
        .expect("white noise returned error");
    let input = Cursor::new(&samples[..]);

    //let input = white_noise::<Complex<f32>>().limit(0x100000);

    let filter = pm_remez(Lowpass::new(0.25, 0.01, 0.05, 0.05), 17)
        .unwrap()
        .fir_filter();

    let filtered_normal = black_box(input.clone()).scan_in_place_with(filter.clone());
    let filtered_single_samples =
        black_box(SingleSampleStream::new(input.clone())).scan_in_place_with(filter);

    group.bench_function("fir normal unbuffered", |b| {
        b.iter(|| {
            read_stream(filtered_normal.clone());
        })
    });

    group.bench_function("fir normal buffered", |b| {
        b.iter(|| {
            read_stream(filtered_normal.clone().buffered(0x4000));
        })
    });

    group.bench_function("fir single-sample unbuffered", |b| {
        b.iter(|| {
            read_stream(filtered_single_samples.clone());
        })
    });

    group.bench_function("fir single-sample buffered", |b| {
        b.iter(|| {
            read_stream(filtered_single_samples.clone().buffered(0x4000));
        })
    });

    group.finish();
}

criterion_group!(benches, bench_buffering);
criterion_main!(benches);

fn read_stream<R, S>(mut stream: R)
where
    R: AsyncReadSamples<S> + Unpin,
{
    loop {
        match stream
            .read_sample()
            .now_or_never()
            .expect("read_sample returned pending")
        {
            Ok(sample) => {
                let _ = black_box(sample);
            }
            Err(EofError::Eof {
                num_samples_read: _,
            }) => break,
            Err(error) => panic!("read_sample returned error: {error:?}"),
        }
    }
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
