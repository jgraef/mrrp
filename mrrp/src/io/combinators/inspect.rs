use std::{
    fmt::Debug,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
    time::{
        Duration,
        Instant,
    },
};

use pin_project_lite::pin_project;
use tracing::Span;

use crate::io::{
    AsyncReadSamples,
    GetSampleRate,
    ReadBuf,
    StreamLength,
};

pin_project! {
    #[derive(Clone, Copy, Debug)]
    pub struct InspectWith<R, I> {
        #[pin]
        inner: R,
        inspector: I,
    }
}

impl<R, I> InspectWith<R, I> {
    #[inline]
    pub fn new(inner: R, inspector: I) -> Self {
        Self { inner, inspector }
    }
}

impl<R, I, S> AsyncReadSamples<S> for InspectWith<R, I>
where
    R: AsyncReadSamples<S>,
    I: Inspector<S>,
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        let this = self.project();

        this.inner
            .poll_read_samples(cx, buffer)
            .map_ok(|()| this.inspector.inspect(buffer.filled()))
    }
}

impl<R, I> GetSampleRate for InspectWith<R, I>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, I> StreamLength for InspectWith<R, I>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}

pin_project! {
    #[derive(Clone, Debug)]
    pub struct Inspect<R, F> {
        #[pin]
        inner: InspectWith<R, FuncInspector<F>>,
    }
}

impl<R, F> Inspect<R, F> {
    #[inline]
    pub fn new(inner: R, f: F) -> Self {
        Self {
            inner: InspectWith::new(inner, FuncInspector::new(f)),
        }
    }
}

impl<R, S, F> AsyncReadSamples<S> for Inspect<R, F>
where
    R: AsyncReadSamples<S>,
    F: FnMut(&[S]),
{
    type Error = R::Error;

    fn poll_read_samples(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<S>,
    ) -> Poll<Result<(), Self::Error>> {
        self.project().inner.poll_read_samples(cx, buffer)
    }
}

impl<R, F> GetSampleRate for Inspect<R, F>
where
    R: GetSampleRate,
{
    #[inline]
    fn sample_rate(&self) -> f32 {
        self.inner.sample_rate()
    }
}

impl<R, F> StreamLength for Inspect<R, F>
where
    R: StreamLength,
{
    #[inline]
    fn remaining(&self) -> usize {
        self.inner.remaining()
    }
}

pub trait Inspector<S> {
    fn inspect(&mut self, samples: &[S]);
}

#[derive(Clone, Debug)]
pub struct FuncInspector<F> {
    f: F,
}

impl<F> FuncInspector<F> {
    #[inline]
    pub fn new(f: F) -> Self {
        Self { f }
    }
}

impl<F, S> Inspector<S> for FuncInspector<F>
where
    F: FnMut(&[S]),
{
    fn inspect(&mut self, samples: &[S]) {
        (self.f)(samples)
    }
}

#[derive(Clone, Debug, Default)]
pub struct LogSampleRateInspector {
    start_time: Option<Instant>,
    reset_time: Option<Duration>,
    num_samples: usize,
    span: Option<Span>,
}

impl LogSampleRateInspector {
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    #[inline]
    pub fn with_reset_time(mut self, reset_time: Duration) -> Self {
        self.reset_time = Some(reset_time);
        self
    }

    #[inline]
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

impl<S> Inspector<S> for LogSampleRateInspector {
    fn inspect(&mut self, samples: &[S]) {
        if let Some(start_time) = self.start_time {
            self.num_samples += samples.len();
            let now = Instant::now();
            let elapsed = now.duration_since(start_time);
            let sample_rate = self.num_samples as f32 / elapsed.as_secs_f32();

            let _guard = self.span.as_ref().map(|span| span.enter());
            tracing::info!("sample rate: {sample_rate:.2} Hz");

            if self
                .reset_time
                .map_or(false, |reset_time| elapsed >= reset_time)
            {
                self.start_time = Some(now);
                self.num_samples = 0;
            }
        }
        else {
            self.start_time = Some(Instant::now());
        }
    }
}

#[derive(Clone, Debug)]
pub struct LogSamplesInspector {
    next_sample: usize,
    interval: usize,
    span: Option<Span>,
}

impl LogSamplesInspector {
    #[inline]
    pub fn new(interval: usize) -> Self {
        Self {
            next_sample: 0,
            interval,
            span: None,
        }
    }

    #[inline]
    pub fn with_span(mut self, span: Span) -> Self {
        self.span = Some(span);
        self
    }
}

impl<S: Debug> Inspector<S> for LogSamplesInspector {
    fn inspect(&mut self, mut samples: &[S]) {
        let _guard = self.span.as_ref().map(|span| span.enter());

        while self.next_sample < samples.len() {
            samples = &samples[self.next_sample..];

            let sample = &samples[0];
            tracing::debug!(?sample);

            self.next_sample = self.interval;
        }
        self.next_sample -= samples.len();
    }
}
