use std::collections::HashMap;

use tokio::sync::{
    mpsc,
    oneshot,
    watch,
};
use uuid::Uuid;

use crate::{
    multiplexer::{
        Error,
        sink::Sink,
        source::Source,
    },
    protocol::{
        StreamId,
        StreamInfo,
    },
};

#[derive(Debug)]
pub enum Command {
    CreateStream {
        source: Box<dyn Source>,
        info_sender: watch::Sender<StreamInfo>,
        response_sender: oneshot::Sender<Result<StreamId, Error>>,
    },
    StopStream {
        stream_id: StreamId,
    },
    Forward {
        stream_id: StreamId,
        sink: Box<dyn Sink>,
        response_sender: oneshot::Sender<Result<(), Error>>,
    },
}

#[derive(Debug)]
pub struct Reactor {
    command_receiver: mpsc::Receiver<Command>,
    streams: HashMap<StreamId, Stream>,
}

impl Reactor {
    #[inline]
    pub fn new(command_receiver: mpsc::Receiver<Command>) -> Self {
        Self {
            command_receiver,
            streams: Default::default(),
        }
    }

    pub async fn run(mut self) {
        while let Some(command) = self.command_receiver.recv().await {
            self.handle_command(command);
        }
    }

    fn handle_command(&mut self, command: Command) {
        let span = tracing::debug_span!("command", ?command);
        let _guard = span.enter();

        match command {
            Command::CreateStream {
                source,
                info_sender,
                response_sender,
            } => {
                let stream_id = self.create_stream(source, info_sender);
                if let Err(_) = response_sender.send(Ok(stream_id)) {
                    // the response receiver has been dropped, so we have to
                    // remove the source again. this should usually not happen,
                    // but better cover the case.
                    tracing::warn!("response receiver dropped");
                    self.stop_stream(stream_id);
                }
            }
            Command::StopStream { stream_id } => {
                self.stop_stream(stream_id);
            }
            Command::Forward {
                stream_id,
                sink,
                response_sender,
            } => {
                // todo: fanout of stream, forward to sink.
                todo!()
            }
        }
    }

    fn create_stream(
        &mut self,
        source: Box<dyn Source>,
        info_sender: watch::Sender<StreamInfo>,
    ) -> StreamId {
        let stream_id = StreamId(Uuid::new_v4());
        self.streams.insert(
            stream_id,
            Stream {
                source,
                info_sender,
            },
        );
        stream_id
    }

    fn stop_stream(&mut self, stream_id: StreamId) {
        if let Some(stream) = self.streams.remove(&stream_id) {
            // todo: here we could signal the stream itself that is has been
            // stopped. for now we just drop it
            drop(stream);
        }
    }
}

#[derive(Debug)]
struct Stream {
    source: Box<dyn Source>,
    info_sender: watch::Sender<StreamInfo>,
}
