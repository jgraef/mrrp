pub mod events;
pub mod stream;

use std::{
    collections::HashMap,
    fmt::Debug,
    num::NonZero,
    sync::Arc,
};

use parking_lot::RwLock;
use tokio_stream::Stream;

use crate::{
    multiplexer::{
        events::{
            EventDispatcher,
            Lagged,
        },
        stream::{
            Source,
            StreamHandle,
            StreamOptions,
        },
    },
    protocol::{
        Event,
        StreamId,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub event_channel_capacity: NonZero<usize>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            event_channel_capacity: NonZero::new(128).unwrap(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Multiplexer {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    state: RwLock<State>,
    event_dispatcher: EventDispatcher,
}

#[derive(Debug)]
struct State {
    streams: HashMap<StreamId, StreamHandle>,
}

impl Default for Multiplexer {
    #[inline]
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl Multiplexer {
    pub fn new(options: Options) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: RwLock::new(State {
                    streams: HashMap::new(),
                }),
                event_dispatcher: EventDispatcher::new(options.event_channel_capacity),
            }),
        }
    }

    pub fn create_stream(
        &self,
        source: impl Into<Box<dyn Source>>,
        options: StreamOptions,
    ) -> StreamHandle {
        let stream_handle = StreamHandle::from_source(source, options);
        self.insert_stream(stream_handle.clone());
        stream_handle
    }

    pub fn insert_stream(&self, stream_handle: StreamHandle) {
        self.shared.event_dispatcher.register_stream(&stream_handle);

        let mut state_guard = self.shared.state.write();
        state_guard
            .streams
            .insert(stream_handle.stream_id(), stream_handle);
    }

    pub fn get_stream(&self, stream_id: StreamId) -> Option<StreamHandle> {
        let state_guard = self.shared.state.read();
        state_guard.streams.get(&stream_id).cloned()
    }

    pub fn list_streams(&self) -> Vec<StreamHandle> {
        let state_guard = self.shared.state.read();
        state_guard.streams.values().cloned().collect()
    }

    #[inline]
    pub fn events(&self) -> impl Stream<Item = Result<Event, Lagged>> {
        self.shared.event_dispatcher.subscribe()
    }
}
