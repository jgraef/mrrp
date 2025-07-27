use std::{
    hint::black_box,
    time::Duration,
};

use criterion::{
    Criterion,
    Throughput,
    criterion_group,
    criterion_main,
};
use futures_util::FutureExt;
use mrrp::{
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
        test::{
            BlackBoxStream,
            SingleSampleStream,
        },
    },
    source::white_noise,
};
use num_complex::Complex;

pub fn bench_buffering(c: &mut Criterion) {
    let num_samples = 0x400000;
    let buffer_size = 0x4000;

    let mut group = c.benchmark_group("buffering");
    group.throughput(Throughput::Elements(num_samples as u64));
    group.measurement_time(Duration::from_secs(20));

    let mut samples = vec![];
    white_noise::<Complex<f32>>()
        .limit(num_samples)
        .read_to_end(&mut samples)
        .now_or_never()
        .expect("white noise returned pending")
        .expect("white noise returned error");
    let input = BlackBoxStream::new(Cursor::new(&samples[..]));

    //let input = white_noise::<Complex<f32>>().limit(0x100000);

    let filter = pm_remez(Lowpass::new(0.25, 0.01, 0.05, 0.05), 17)
        .unwrap()
        .fir_filter();

    group.bench_function("fir normal unbuffered", |b| {
        b.iter(|| {
            read_stream(input.clone().scan_in_place_with(filter.clone()));
        })
    });

    group.bench_function("fir normal buffered", |b| {
        b.iter(|| {
            read_stream(
                input
                    .clone()
                    .scan_in_place_with(filter.clone())
                    .buffered(buffer_size),
            );
        })
    });

    group.bench_function("fir single-sample unbuffered", |b| {
        b.iter(|| {
            read_stream(SingleSampleStream::new(input.clone()).scan_in_place_with(filter.clone()));
        })
    });

    group.bench_function("fir single-sample buffered", |b| {
        b.iter(|| {
            read_stream(
                SingleSampleStream::new(input.clone())
                    .scan_in_place_with(filter.clone())
                    .buffered(buffer_size),
            );
        })
    });

    group.finish();
}

criterion_group!(benches, bench_buffering);
criterion_main!(benches);

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
