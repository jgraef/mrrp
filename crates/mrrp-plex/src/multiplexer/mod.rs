pub mod events;
pub mod sources;
pub mod stream;

use std::{
    collections::HashMap,
    fmt::Debug,
    num::NonZero,
    sync::Arc,
};

use parking_lot::RwLock;

use crate::{
    multiplexer::{
        events::{
            EventBroadcaster,
            Events,
        },
        sources::SourceScanners,
        stream::{
            Source,
            StreamHandle,
            StreamOptions,
        },
    },
    protocol::StreamId,
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
    source_scanners: SourceScanners,
    event_broadcaster: EventBroadcaster,
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
                source_scanners: SourceScanners::default(),
                event_broadcaster: EventBroadcaster::new(options.event_channel_capacity),
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
        // todo: this is a bit silly, but we kind of don't want this method to
        // be async
        tokio::spawn({
            let mut command_sender = stream_handle.stream_control().command_sender().clone();
            let event_broadcaster = self.shared.event_broadcaster.clone();

            async move {
                command_sender
                    .set_event_broadcaster(Some(event_broadcaster))
                    .await
                    .unwrap();
            }
        });

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

    pub fn events(&self) -> Events {
        self.shared.event_broadcaster.subscribe()
    }
}
