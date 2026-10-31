use std::{
    fmt::Debug,
    num::NonZero,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::{
    FutureExt,
    Stream,
    TryStreamExt,
    pin_mut,
};
use tokio::sync::{
    broadcast,
    mpsc,
};
use tokio_stream::wrappers::{
    BroadcastStream,
    errors::BroadcastStreamRecvError,
};

use crate::{
    multiplexer::stream::{
        Closed,
        StateReceiver,
        StreamHandle,
    },
    protocol::{
        Event,
        StreamEvent,
        StreamId,
        StreamInfo,
    },
};

#[derive(Debug)]
pub struct EventDispatcher {
    event_sender: broadcast::Sender<Event>,
    command_sender: mpsc::UnboundedSender<Command>,
}

impl EventDispatcher {
    pub fn new(event_channel_capacity: NonZero<usize>) -> Self {
        let event_sender = broadcast::Sender::new(event_channel_capacity.get());
        let (command_sender, command_receiver) = mpsc::unbounded_channel();

        let reactor = Reactor::new(command_receiver, event_sender.clone());
        tokio::spawn(reactor.run());

        Self {
            event_sender,
            command_sender,
        }
    }

    pub fn subscribe(&self) -> impl Stream<Item = Result<Event, Lagged>> {
        BroadcastStream::new(self.event_sender.subscribe())
            .map_err(|BroadcastStreamRecvError::Lagged(lag)| Lagged { lag })
    }

    pub fn register_stream(&self, stream_handle: &StreamHandle) {
        let stream_id = stream_handle.stream_id();
        let state_receiver = stream_handle.stream_control().state_receiver().clone();

        self.command_sender
            .send(Command::RegisterStream {
                stream_id,
                state_receiver,
            })
            .unwrap();
    }
}

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Receiver lagging behind: {lag}")]
pub struct Lagged {
    pub lag: u64,
}

#[derive(Debug)]
enum Command {
    RegisterStream {
        stream_id: StreamId,
        state_receiver: StateReceiver,
    },
}

/// Helper to merge stream info updates from multiple StateReceivers into a
/// single future.
///
/// Note that if there are no receivers to be polled, this will still return
/// `Poll::Pending` and thus be pending forever.
#[derive(Debug)]
struct PollAllStateReceivers<'a> {
    tracked_streams: &'a mut Vec<TrackedStream>,
    buffer: &'a mut Vec<Event>,
}

impl<'a> PollAllStateReceivers<'a> {
    pub fn new(tracked_streams: &'a mut Vec<TrackedStream>, buffer: &'a mut Vec<Event>) -> Self {
        Self {
            tracked_streams,
            buffer,
        }
    }
}

impl<'a> Future for PollAllStateReceivers<'a> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        assert!(self.buffer.is_empty());

        let this = &mut *self;

        let mut all_pending = true;

        this.tracked_streams.retain_mut(|tracked_stream| {
            let poll = {
                let fut = tracked_stream
                    .state_receiver
                    .wait_stream_info(|stream_info| Some(stream_info.clone()));
                pin_mut!(fut);
                fut.poll_unpin(cx)
            };

            match poll {
                Poll::Pending => true,
                Poll::Ready(Err(Closed)) => {
                    all_pending = false;
                    false
                }
                Poll::Ready(Ok(stream_info)) => {
                    tracked_stream.generate_events(stream_info, &mut this.buffer);
                    all_pending = false;
                    true
                }
            }
        });

        if all_pending {
            // when all receivers are pending, there's no need to iterate
            // again and poll all gain. so we'll return pending too.
            //
            // if there's at least one receiver, that one is pending, so
            // we'll have a waker registered and will be woken up again.
            //
            // if there are no receivers, we don't want this future to ever
            // return anyway. this is because the select we do
            // below would spin otherwise.

            assert!(this.buffer.is_empty());
            Poll::Pending
        }
        else {
            Poll::Ready(())
        }
    }
}

#[derive(Debug)]
struct TrackedStream {
    stream_id: StreamId,
    state_receiver: StateReceiver,
    stream_info: StreamInfo,
}

impl TrackedStream {
    fn generate_events(&mut self, stream_info: StreamInfo, output: &mut Vec<Event>) {
        if stream_info != self.stream_info {
            self.stream_info = stream_info.clone();
            output.push(Event::StreamEvent {
                stream_id: self.stream_id,
                event: StreamEvent::InfoChanged { stream_info },
            })
        }
    }
}

struct Reactor {
    command_receiver: mpsc::UnboundedReceiver<Command>,
    event_sender: broadcast::Sender<Event>,
    buffer: Vec<Event>,
    tracked_streams: Vec<TrackedStream>,
}

impl Reactor {
    fn new(
        command_receiver: mpsc::UnboundedReceiver<Command>,
        event_sender: broadcast::Sender<Event>,
    ) -> Self {
        Self {
            command_receiver,
            event_sender,
            buffer: vec![],
            tracked_streams: vec![],
        }
    }

    async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                option = self.command_receiver.recv() => {
                    let Some(command) = option else { break; };
                    self.handle_command(command);
                }
                _ = PollAllStateReceivers::new(&mut self.tracked_streams, &mut self.buffer) => {
                    for event in self.buffer.drain(..) {
                        if let Err(_) = self.event_sender.send(event) {
                            // no receivers left
                            break;
                        }
                    }

                }
            }
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::RegisterStream {
                stream_id,
                state_receiver,
            } => {
                let stream_info = state_receiver.stream_info();
                self.tracked_streams.push(TrackedStream {
                    stream_id,
                    state_receiver,
                    stream_info,
                });
            }
        }
    }
}
