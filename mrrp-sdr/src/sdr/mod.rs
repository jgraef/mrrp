pub mod sink;
pub mod source;

use std::{
    collections::HashMap,
    pin::Pin,
    sync::{
        Arc,
        atomic::{
            AtomicBool,
            Ordering,
        },
    },
    task::{
        Context,
        Poll,
        Waker,
    },
};

use anyhow::Error;
use mrrp::{
    buf::{
        SampleBufMut,
        SamplesMut,
    },
    io::AsyncReadSamples,
};
use num_complex::Complex;
use rustfft::FftPlanner;
use tokio::sync::mpsc;
use tracing::Instrument;

use crate::{
    sdr::{
        sink::{
            RepaintOnPush,
            SpectrumFrame,
            SpectrumSink,
        },
        source::Source,
    },
    util::AtomicIds,
};

pub type Iq = Complex<f32>;

#[derive(derive_more::Debug)]
pub struct SdrRuntime {
    command_receiver: mpsc::UnboundedReceiver<Command>,

    buffer_size: usize,
    amplitude_buffer: SamplesMut<f32>,
    fft: Fft,

    sources: Sources,

    #[debug(skip)]
    spectrum_sinks: HashMap<usize, Box<dyn SpectrumSink + Send>>,
}

impl SdrRuntime {
    pub fn spawn() -> SdrHandle {
        let (command_sender, command_receiver) = mpsc::unbounded_channel();

        // todo: don't hard-code. this is also the window_size for the FFT
        let buffer_size = 4096;

        let this = Self {
            command_receiver,
            buffer_size,
            amplitude_buffer: SamplesMut::with_capacity(buffer_size),
            fft: Fft::default(),
            sources: Sources::new(buffer_size),
            spectrum_sinks: HashMap::new(),
        };

        let span = tracing::info_span!("sdr-runtime");

        let _join_handle = tokio::spawn(this.run().instrument(span));

        SdrHandle {
            command_sender,
            handle_ids: Default::default(),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                command = self.command_receiver.recv() => {
                    let Some(command) = command else { break; };
                    self.handle_command(command);
                }
                id = self.sources.read() => {
                    self.handle_data(id);
                }
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        tracing::debug!(?command, "handling command");

        match command {
            Command::AddSource { id, source } => {
                self.sources.insert(id, source);
            }
            Command::RemoveSource { id } => {
                self.sources.remove(id);
            }
            Command::AddSpectrumSink {
                id,
                spectrum_sink: sink,
            } => {
                self.spectrum_sinks.insert(id, sink);
            }
            Command::RemoveSpectrumSink { id } => {
                self.spectrum_sinks.remove(&id);
            }
        }
    }

    fn handle_data(&mut self, id: usize) {
        let buffered_source = &mut self
            .sources
            .buffered_sources
            .get_mut(&id)
            .unwrap_or_else(|| panic!("Got data for source #{id}, but it doesn't exist anymore."));

        assert_eq!(buffered_source.buffer.len(), self.buffer_size);

        let source_info = buffered_source.source.as_mut().info();

        // calculate FFT of signal (in-place)
        //
        // todo: we really should do this on a separate thread.
        //
        // we would need to be able to send this buffer to another thread. options:
        //
        // 1. immediately copy to another buffer
        // 2. steal buffer. could use a double-buffer (though this might to work well),
        //    or a buffer-pool. once the stolen buffer is free, we can put it into the
        //    pool. buffers that were stolen get replaced with buffers from the pool.
        //    but at that point we might just use the system allocator as a pool lol
        // 3. put buffer into a RefCell. while reading sources we would just skip any
        //    source whose buffer is being fft'd
        //
        // i guess this is one of those cases where we just have to look at the
        // performance instead of guessing which works best.
        self.fft.process(&mut buffered_source.buffer);

        // calculate power
        self.amplitude_buffer.clear();
        self.amplitude_buffer.extend(
            buffered_source
                .buffer
                .iter()
                .map(|sample| sample.norm_sqr()),
        );

        let frame = SpectrumFrame {
            center_frequency: source_info.center_frequency,
            sample_rate: source_info.sample_rate,
            data: &*self.amplitude_buffer,
        };

        for (_, spectrum_sink) in &mut self.spectrum_sinks {
            spectrum_sink.push(frame);
        }

        // clear buffer
        buffered_source.buffer.clear();
    }
}

#[derive(Debug)]
struct Sources {
    buffer_size: usize,
    buffered_sources: HashMap<usize, BufferedSource>,
    waker: Waker,
}

impl Sources {
    pub fn new(buffer_size: usize) -> Self {
        Self {
            buffer_size,
            buffered_sources: HashMap::new(),
            waker: Waker::noop().clone(),
        }
    }

    pub fn insert(&mut self, id: usize, source: Pin<Box<dyn Source + Send>>) {
        self.buffered_sources.insert(
            id,
            BufferedSource {
                source,
                buffer: SamplesMut::with_capacity(self.buffer_size),
            },
        );

        // a task might be waiting for data, but will not be woken by any of the exiting
        // sources (if there are any). so we also need to wake if there is a new souce.
        self.waker.wake_by_ref();
    }

    pub fn remove(&mut self, id: usize) {
        self.buffered_sources.remove(&id);
    }

    pub fn read(&mut self) -> ReadSources<'_> {
        ReadSources { sources: self }
    }
}

#[derive(derive_more::Debug)]
struct BufferedSource {
    #[debug(skip)]
    source: Pin<Box<dyn Source + Send>>,

    buffer: SamplesMut<Iq>,
}

/// Helper to read from all sources at once
#[derive(Debug)]
struct ReadSources<'a> {
    sources: &'a mut Sources,
}

impl<'a> Future for ReadSources<'a> {
    type Output = usize;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let sources = &mut *self.sources;

        'outer: loop {
            let mut all_pending = true;

            for (&id, buffered_source) in &mut sources.buffered_sources {
                // todo: this expect might fail if we reduce the requested amount between calls
                // to this. we should handle this case.
                let mut remaining_buffer_capacity = sources
                    .buffer_size
                    .checked_sub(buffered_source.buffer.len())
                    .expect("buffer overflowed");

                assert!(remaining_buffer_capacity != 0, "buffer full");

                let (result, num_read) = buffered_source.buffer.with_read_buf(|read_buf| {
                    buffered_source
                        .source
                        .as_mut()
                        .poll_read_samples(cx, read_buf)
                });
                assert!(num_read <= sources.buffer_size);

                match result {
                    Poll::Pending => {
                        // source pending, try next
                    }
                    Poll::Ready(Err(error)) => {
                        // the source failed. we can't remove it while we're iterating over it, so
                        // we remove it and start over
                        tracing::error!(id, %error, "Source error");
                        sources.buffered_sources.remove(&id);
                        continue 'outer;
                    }
                    Poll::Ready(Ok(())) => {
                        // got some samples.

                        if num_read == 0 {
                            // nevermind, this is the end of stream
                            tracing::error!(id, "Source end of stream");
                            sources.buffered_sources.remove(&id);
                            continue 'outer;
                        }

                        // check if the buffer has been filled
                        remaining_buffer_capacity -= num_read;
                        if remaining_buffer_capacity == 0 {
                            // would be nice if we could return the source and/or buffer here too,
                            // but we don't think we can.

                            return Poll::Ready(id);
                        }
                        else {
                            // not full yet, so just go to the next
                            all_pending = false;
                        }
                    }
                }
            }

            if all_pending {
                // all sources pending, so we just return pending.
                // note that there might be no sources at all.
                // in either case we need to wake if we get a new source
                self.sources.waker.clone_from(cx.waker());

                return Poll::Pending;
            }
            else {
                // at least one source returned data, but didn't fill the
                // buffer. try again
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct SdrHandle {
    command_sender: mpsc::UnboundedSender<Command>,
    handle_ids: Arc<AtomicIds>,
}

impl SdrHandle {
    fn send_command(&self, command: Command) {
        self.command_sender
            .send(command)
            .expect("SDR runtime command channel closed");
    }

    pub fn add_spectrum_sink<S>(&self, sink: S) -> SpectrumSinkHandle
    where
        S: SpectrumSink + Send + 'static,
    {
        let id = self.handle_ids.next();

        self.send_command(Command::AddSpectrumSink {
            id,
            spectrum_sink: Box::new(sink),
        });

        SpectrumSinkHandle::new(self.command_sender.clone(), id)
    }

    #[must_use]
    pub fn add_source<S>(&self, source: S) -> SourceHandle
    where
        S: Source + Sized + Send + 'static,
        <S as AsyncReadSamples<Iq>>::Error: Into<Error> + Sized + Send + Sync + 'static,
    {
        let id = self.handle_ids.next();

        self.send_command(Command::AddSource {
            id,
            source: Box::pin(source),
        });

        SourceHandle::new(self.command_sender.clone(), id)
    }
}

#[derive(derive_more::Debug)]
enum Command {
    AddSource {
        id: usize,
        #[debug(skip)]
        source: Pin<Box<dyn Source + Send>>,
    },
    RemoveSource {
        id: usize,
    },
    AddSpectrumSink {
        id: usize,
        #[debug(skip)]
        spectrum_sink: Box<dyn SpectrumSink + Send>,
    },
    RemoveSpectrumSink {
        id: usize,
    },
}

#[derive(Debug)]
struct HandleInner {
    command_sender: mpsc::UnboundedSender<Command>,
    id: usize,
    remove_on_drop: AtomicBool,
    on_drop: fn(usize) -> Command,
}

impl HandleInner {
    fn new(
        command_sender: mpsc::UnboundedSender<Command>,
        id: usize,
        on_drop: fn(usize) -> Command,
    ) -> Self {
        Self {
            command_sender,
            id,
            remove_on_drop: AtomicBool::new(true),
            on_drop,
        }
    }

    fn leak(&self) {
        self.remove_on_drop.store(false, Ordering::Relaxed);
    }
}

impl Drop for HandleInner {
    fn drop(&mut self) {
        let command = (self.on_drop)(self.id);

        if self.remove_on_drop.load(Ordering::Relaxed) {
            let _ = self.command_sender.send(command);
        }
        else {
            tracing::debug!(?command, "leaking handle");
        }
    }
}

#[derive(Clone, Debug)]
pub struct SpectrumSinkHandle {
    inner: Arc<HandleInner>,
}

impl SpectrumSinkHandle {
    fn new(command_sender: mpsc::UnboundedSender<Command>, id: usize) -> Self {
        Self {
            inner: Arc::new(HandleInner::new(command_sender, id, |id| {
                Command::RemoveSpectrumSink { id }
            })),
        }
    }

    pub fn leak(self) {
        self.inner.leak();
    }
}

#[derive(Clone, Debug)]
pub struct SourceHandle {
    inner: Arc<HandleInner>,
}

impl SourceHandle {
    fn new(command_sender: mpsc::UnboundedSender<Command>, id: usize) -> Self {
        Self {
            inner: Arc::new(HandleInner::new(command_sender, id, |id| {
                Command::RemoveSource { id }
            })),
        }
    }

    pub fn leak(self) {
        self.inner.leak();
    }
}

pub trait GetSdrHandle {
    fn sdr_handle(&self) -> Option<SdrHandle>;

    fn expect_sdr_handle(&self) -> SdrHandle {
        self.sdr_handle()
            .expect("Could not retrieve handle to SDR runtime")
    }
}

impl GetSdrHandle for egui::Context {
    fn sdr_handle(&self) -> Option<SdrHandle> {
        self.data(|data| data.get_temp(egui::Id::NULL))
    }
}

pub fn initialize_sdr_runtime(ctx: &egui::Context) -> SdrHandle {
    let sdr_handle = SdrRuntime::spawn();
    ctx.data_mut(|data| data.insert_temp(egui::Id::NULL, sdr_handle.clone()));
    sdr_handle
}

pub fn ensure_spectrum_sink_is_linked<S>(
    ctx: &egui::Context,
    sink: &S,
    handle: &mut Option<SpectrumSinkHandle>,
) where
    S: SpectrumSink + Send + Clone + 'static,
{
    if handle.is_none() {
        let sdr = ctx.expect_sdr_handle();
        *handle = Some(sdr.add_spectrum_sink(RepaintOnPush::new(sink.clone(), ctx.clone())));
    }
}

#[derive(derive_more::Debug)]
struct Fft {
    #[debug(skip)]
    planner: FftPlanner<f32>,

    scratch: Vec<Complex<f32>>,
}

impl Default for Fft {
    fn default() -> Self {
        Self {
            planner: FftPlanner::new(),
            scratch: vec![],
        }
    }
}

impl Fft {
    pub fn process(&mut self, data: &mut [Complex<f32>]) {
        let fft = self.planner.plan_fft_forward(data.len());

        let scratch_len = fft.get_inplace_scratch_len();
        self.scratch.resize(scratch_len, Default::default());

        fft.process_with_scratch(data, &mut self.scratch);
    }
}
