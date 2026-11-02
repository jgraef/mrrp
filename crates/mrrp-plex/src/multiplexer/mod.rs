pub mod events;
pub mod source;
pub mod stream;

use std::{
    collections::HashMap,
    fmt::Debug,
    sync::Arc,
};

use parking_lot::RwLock;

use crate::{
    multiplexer::stream::StreamHandle,
    protocol::StreamId,
};

#[derive(Clone, Debug)]
pub struct Multiplexer {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    state: RwLock<State>,
}

#[derive(Debug)]
struct State {
    streams: HashMap<StreamId, StreamHandle>,
}

impl Default for Multiplexer {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Multiplexer {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                state: RwLock::new(State {
                    streams: HashMap::new(),
                }),
            }),
        }
    }

    pub fn insert_stream(&self, stream_handle: StreamHandle) {
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
}
