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
    Stream,
    StreamExt,
};
use tokio::sync::broadcast;
use tokio_stream::wrappers::{
    BroadcastStream,
    errors::BroadcastStreamRecvError,
};

use crate::protocol::Event;

#[derive(Clone, Copy, Debug, thiserror::Error)]
#[error("Receiver lagging behind: {lag}")]
pub struct Lagged {
    pub lag: u64,
}

#[derive(Debug)]
pub struct Events {
    stream: BroadcastStream<Event>,
}

impl Stream for Events {
    type Item = Result<Event, Lagged>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.stream.poll_next_unpin(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Ready(Some(Err(BroadcastStreamRecvError::Lagged(lag)))) => {
                Poll::Ready(Some(Err(Lagged { lag })))
            }
            Poll::Ready(Some(Ok(event))) => Poll::Ready(Some(Ok(event))),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EventBroadcaster {
    sender: broadcast::Sender<Event>,
}

impl Default for EventBroadcaster {
    fn default() -> Self {
        Self::new(NonZero::new(128).unwrap())
    }
}

impl EventBroadcaster {
    #[inline]
    pub fn new(capacity: NonZero<usize>) -> Self {
        Self {
            sender: broadcast::Sender::new(capacity.get()),
        }
    }

    #[inline]
    pub fn subscribe(&self) -> Events {
        Events {
            stream: BroadcastStream::new(self.sender.subscribe()),
        }
    }

    #[inline]
    pub fn send(&self, event: Event) {
        // we don't care if the channel is closed. this usually only happens
        // when the `Multiplexer` is shutdown and then also the command senders
        // are dropped.
        self.sender.send(event).ok();
    }
}
