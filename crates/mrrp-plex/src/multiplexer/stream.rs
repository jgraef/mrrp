#![allow(dead_code)]

use std::{
    fmt::Debug,
    num::NonZero,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use mrrp_core::{
    sample::Complex,
    signal::{
        AsyncReadSamples,
        GetSampleRate,
        ReadBuf,
        Remaining,
        SizeHint,
        StreamLength,
    },
};
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    broadcast,
};
use tokio::sync::{
    mpsc,
    watch,
};
use tracing::Instrument;

use crate::protocol::{
    StreamId,
    StreamInfo,
};

pub type SourceError = Box<dyn std::error::Error + Send + Sync + 'static>;

// todo: add `Seek + Pause`, and maybe `GetSampleRate`, etc.
pub trait Source:
    AsyncReadSamples<Sample = Complex<f32>, Error = SourceError>
    + GetSampleRate
    + Debug
    + Send
    + Sync
    + Unpin
    + 'static
{
}

impl<T> Source for T where
    T: AsyncReadSamples<Sample = Complex<f32>, Error = SourceError>
        + GetSampleRate
        + Debug
        + Send
        + Sync
        + Unpin
        + 'static
{
}

#[derive(Clone, Debug)]
pub struct StreamOptions {
    pub broadcast_buffer_size: NonZero<usize>,
    pub read_buffer_size: NonZero<usize>,
    pub command_queue_capacity: NonZero<usize>,
}

impl Default for StreamOptions {
    fn default() -> Self {
        Self {
            broadcast_buffer_size: NonZero::new(0x10000).unwrap(),
            read_buffer_size: NonZero::new(0x8000).unwrap(),
            command_queue_capacity: NonZero::new(64).unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct StreamHandle {
    stream_id: StreamId,
    stream_control: StreamControl,
    samples_subscriber: broadcast::Subscriber<Complex<f32>>,
}

impl StreamHandle {
    pub fn from_source(source: impl Into<Box<dyn Source>>, options: StreamOptions) -> StreamHandle {
        let source = source.into();
        let stream_id = StreamId::generate();

        // create initial StreamInfo
        let stream_info = StreamInfo {
            center_frequency: None,
            sample_rate: source.sample_rate(),
        };

        // create channels
        let (state_sender, state_receiver) = watch::channel(State {
            stream_info,
            error: None,
        });
        let (command_sender, command_receiver) =
            mpsc::channel(options.command_queue_capacity.get());
        let (samples_sender, samples_subscriber) = broadcast::pipe(options.broadcast_buffer_size);

        // spawn stream task
        let span = tracing::debug_span!("stream", ?stream_id);
        let stream_task = StreamTask::new(
            source,
            state_sender,
            command_receiver,
            samples_sender,
            options.read_buffer_size,
        );
        tokio::spawn(stream_task.run().instrument(span));

        Self {
            stream_id,
            stream_control: StreamControl {
                state_receiver: StateReceiver { state_receiver },
                command_sender: CommandSender { command_sender },
            },
            samples_subscriber,
        }
    }

    #[inline]
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[inline]
    pub fn stream_control(&self) -> &StreamControl {
        &self.stream_control
    }

    #[inline]
    pub async fn wait_stream_info<R>(
        &mut self,
        f: impl FnMut(&StreamInfo) -> Option<R>,
    ) -> Result<R, Closed> {
        self.stream_control.wait_stream_info(f).await
    }

    #[inline]
    pub fn stream_info(&self) -> StreamInfo {
        self.stream_control.stream_info()
    }

    pub fn subscribe(&self) -> Result<Stream, Closed> {
        let samples_receiver = self
            .samples_subscriber
            .subscribe()
            .map_err(|broadcast::NoSenders| Closed)?;

        Ok(Stream {
            stream_id: self.stream_id,
            stream_control: self.stream_control.clone(),
            stream_reader: StreamReader { samples_receiver },
        })
    }
}

#[derive(Debug)]
pub struct Stream {
    stream_id: StreamId,
    stream_control: StreamControl,
    stream_reader: StreamReader,
}

impl Stream {
    #[inline]
    pub fn stream_handle(&self) -> StreamHandle {
        StreamHandle {
            stream_id: self.stream_id,
            stream_control: self.stream_control.clone(),
            samples_subscriber: self.stream_reader.samples_receiver.subscriber(),
        }
    }

    #[inline]
    pub fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[inline]
    pub fn stream_control(&self) -> &StreamControl {
        &self.stream_control
    }

    #[inline]
    pub fn stream_reader(&self) -> &StreamReader {
        &self.stream_reader
    }

    #[inline]
    pub fn split(self) -> (StreamControl, StreamReader) {
        (self.stream_control, self.stream_reader)
    }

    #[inline]
    pub fn stream_info(&self) -> StreamInfo {
        self.stream_control.stream_info()
    }

    #[inline]
    pub async fn wait_stream_info<R>(
        &mut self,
        f: impl FnMut(&StreamInfo) -> Option<R>,
    ) -> Result<R, Closed> {
        self.stream_control.wait_stream_info(f).await
    }

    // todo: pause, resume, and other aux commands
}

impl AsyncReadSamples for Stream {
    type Sample = Complex<f32>;
    type Error = !;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        self.stream_reader.poll_read_samples_unpin(cx, buffer)
    }
}

impl StreamLength for Stream {
    #[inline]
    fn remaining(&self) -> Remaining {
        self.stream_reader.remaining()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        self.stream_reader.size_hint()
    }
}

#[derive(Clone, Debug)]
pub struct StreamControl {
    command_sender: CommandSender,
    state_receiver: StateReceiver,
}

impl StreamControl {
    #[inline]
    pub fn stream_info(&self) -> StreamInfo {
        self.state_receiver.stream_info()
    }

    #[inline]
    pub fn command_sender(&self) -> &CommandSender {
        &self.command_sender
    }

    #[inline]
    pub fn state_receiver(&self) -> &StateReceiver {
        &self.state_receiver
    }

    pub fn split(self) -> (CommandSender, StateReceiver) {
        (self.command_sender, self.state_receiver)
    }

    #[inline]
    pub async fn wait_stream_info<R>(
        &mut self,
        f: impl FnMut(&StreamInfo) -> Option<R>,
    ) -> Result<R, Closed> {
        self.state_receiver.wait_stream_info(f).await
    }
}

#[derive(Clone, Debug)]
pub struct CommandSender {
    command_sender: mpsc::Sender<Command>,
}

impl CommandSender {
    #[inline]
    async fn send_command(&self, command: Command) -> Result<(), Closed> {
        self.command_sender.send(command).await.map_err(|_| Closed)
    }
}

#[derive(Clone, Debug)]
pub struct StateReceiver {
    state_receiver: watch::Receiver<State>,
}

impl StateReceiver {
    #[inline]
    pub fn stream_info(&self) -> StreamInfo {
        let state_guard = self.state_receiver.borrow();
        state_guard.stream_info.clone()
    }

    /// Wait for a stream info update
    ///
    /// The provided closure can be used for filtering and mapping.
    ///
    /// # Cancel safety
    ///
    /// This method is cancel safe.
    pub async fn wait_stream_info<R>(
        &mut self,
        mut f: impl FnMut(&StreamInfo) -> Option<R>,
    ) -> Result<R, Closed> {
        loop {
            self.state_receiver.changed().await.map_err(|_| Closed)?;
            let state_guard = self.state_receiver.borrow_and_update();
            if let Some(output) = f(&state_guard.stream_info) {
                return Ok(output);
            }
        }
    }
}

/*
impl futures_util::Stream for StateReceiver {
    type Item = StreamInfo;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        {
            let fut = self.state_receiver.changed();
            pin_mut!(fut);
            if let Err(_error) = ready!(fut.poll_unpin(cx)) {
                return Poll::Ready(None);
            }
        }

        let state = self.state_receiver.borrow_and_update();
        Poll::Ready(Some(state.stream_info.clone()))
    }
}
*/

#[derive(Debug)]
pub struct StreamReader {
    samples_receiver: broadcast::Receiver<Complex<f32>>,
}

impl AsyncReadSamples for StreamReader {
    type Sample = Complex<f32>;
    type Error = !;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        self.samples_receiver.poll_read_samples_unpin(cx, buffer)
    }
}

impl StreamLength for StreamReader {
    #[inline]
    fn remaining(&self) -> Remaining {
        self.samples_receiver.remaining()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        self.samples_receiver.size_hint()
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("The stream has been closed")]
pub struct Closed;

#[derive(Debug)]
struct State {
    stream_info: StreamInfo,
    error: Option<SourceError>,
}

#[derive(Debug)]
enum Command {
    Stop,
    Pause,
    Resume,
    Seek { position: usize },
    SetCenterFrequency { center_frequency: f32 },
    SetSampleRate { sample_rate: f32 },
}

#[derive(Debug)]
struct StreamTask {
    source: Box<dyn Source>,
    state_sender: watch::Sender<State>,
    command_receiver: mpsc::Receiver<Command>,
    samples_sender: broadcast::Sender<Complex<f32>>,
    read_buffer: Vec<Complex<f32>>,
    manually_paused: bool,
    no_receivers: bool,
}

impl StreamTask {
    fn new(
        source: Box<dyn Source>,
        state_sender: watch::Sender<State>,
        command_receiver: mpsc::Receiver<Command>,
        samples_sender: broadcast::Sender<Complex<f32>>,
        read_buffer_size: NonZero<usize>,
    ) -> Self {
        let read_buffer = vec![Complex::ZERO; read_buffer_size.get()];

        Self {
            source,
            state_sender,
            command_receiver,
            samples_sender,
            read_buffer,
            manually_paused: false,
            no_receivers: true,
        }
    }

    async fn run(mut self) {
        if let Err(error) = self.run_inner().await {
            tracing::error!(%error);
            self.state_sender
                .send_modify(move |state| state.error = Some(error));
        }
    }

    async fn run_inner(&mut self) -> Result<(), SourceError> {
        loop {
            tokio::select! {
                biased;

                // always handle commands first
                option = self.command_receiver.recv() => {
                    let Some(command) = option else { break; };

                    tracing::debug!(?command);

                    match command {
                        Command::Stop => {
                            break;
                        },
                        Command::Pause => {
                            self.manually_paused = true;
                            self.pause_source().await?;
                        }
                        Command::Resume => {
                            self.manually_paused = false;
                            self.resume_source().await?;
                        }
                        Command::Seek { position } => {
                            // todo: needs #44
                            tracing::debug!(?position, "todo: seek");
                        }
                        Command::SetCenterFrequency { center_frequency } => {
                            tracing::debug!(?center_frequency, "todo: set center frequency");
                        },
                        Command::SetSampleRate { sample_rate } => {
                            tracing::debug!(?sample_rate, "todo: set sample rate");
                        },
                    }
                }

                // possibly wait for receivers
                result = self.samples_sender.wait_for_receivers(), if self.no_receivers => {
                    if let Err(broadcast::NoSubscribers) = result {
                        // all subscribers dropped, and no receivers, so we can stop.
                        break;
                    }

                    self.no_receivers = false;

                    if !self.manually_paused {
                        tracing::debug!("New receivers, resuming source");
                        self.resume_source().await?;
                    }
                }

                // read samples from source and write them into the broadcast pipe, if we're not paused
                result = self.source.read_samples(&mut self.read_buffer), if !self.no_receivers && !self.manually_paused => {
                    let num_samples = result?;

                    if num_samples == 0 {
                        // source is EOF.
                        break;
                    }

                    if let Err(broadcast::NoReceivers) = self.samples_sender.send(&self.read_buffer[..num_samples]) {
                        tracing::debug!("No receivers, pausing source");
                        self.no_receivers = true;
                        self.pause_source().await?;
                    }
                }

            }
        }

        Ok(())
    }

    async fn pause_source(&mut self) -> Result<(), SourceError> {
        // todo: tell source to pause (#51)

        Ok(())
    }

    async fn resume_source(&mut self) -> Result<(), SourceError> {
        // todo: tell source to resume (#51)

        Ok(())
    }
}
