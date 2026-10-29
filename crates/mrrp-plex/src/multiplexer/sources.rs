use std::fmt::Debug;

use tokio::sync::broadcast;

type Id = usize;

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub command_channel_capacity: usize,
    pub event_channel_capacity: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            command_channel_capacity: 64,
            event_channel_capacity: 64,
        }
    }
}

#[derive(Debug)]
pub(super) struct SourceScanners {
    command_sender: broadcast::Sender<(Option<Id>, Command)>,
    event_sender: broadcast::Sender<(Id, Event)>,
    event_receiver: broadcast::Receiver<(Id, Event)>,
}

impl SourceScanners {
    pub fn new(options: Options) -> Self {
        let command_sender = broadcast::Sender::new(options.command_channel_capacity);
        let (event_sender, event_receiver) = broadcast::channel(options.event_channel_capacity);
        Self {
            command_sender,
            event_sender,
            event_receiver,
        }
    }
}

impl Default for SourceScanners {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

pub struct Socket {
    id: Id,
    command_receiver: broadcast::Receiver<(Option<Id>, Command)>,
    event_sender: broadcast::Sender<(Id, Event)>,
}

impl Socket {
    pub async fn next_command(&mut self) -> Option<Command> {
        loop {
            match self.command_receiver.recv().await {
                Ok((id, command)) => {
                    if id.is_none_or(|id| id == self.id) {
                        return Some(command);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(lag)) => {
                    tracing::warn!(?lag, "Source scanner command receiver is lagging behind");
                }
                Err(broadcast::error::RecvError::Closed) => {
                    return None;
                }
            }
        }
    }

    pub fn send_event(&mut self, event: Event) {
        // note: we ignore errors here. send returns an error when there are no
        // receivers, but then there should be no senders for the command
        // channel left either. the scanner should terminate when the command
        // channel returns `None`.
        self.event_sender.send((self.id, event)).ok();
    }
}

#[derive(Clone, Debug)]
pub enum Command {}

#[derive(Clone, Debug)]
pub enum Event {}
