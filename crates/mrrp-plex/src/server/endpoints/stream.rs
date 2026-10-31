use std::{
    fmt::Display,
    num::NonZero,
    pin::Pin,
    task::{
        Poll,
        ready,
    },
};

use axum::{
    extract::{
        Path,
        State,
        WebSocketUpgrade,
        ws::{
            CloseFrame,
            Message,
            WebSocket,
        },
    },
    response::{
        IntoResponse,
        Response,
    },
};
use bytes::{
    Buf,
    BufMut,
    Bytes,
    BytesMut,
};
use mrrp_core::sample::{
    Complex,
    encoding::BigEndian,
};
use mrrp_format::pcm::PcmWriter;
use mrrp_util::signal::{
    AsyncReadSamplesExt,
    ForwardError,
};
use serde::{
    Serialize,
    de::DeserializeOwned,
};
use tokio::{
    io::AsyncWrite,
    sync::mpsc,
};
use tokio_util::sync::PollSender;

use crate::{
    multiplexer::stream::StreamHandle,
    protocol::{
        StreamCommand,
        StreamEvent,
        StreamId,
        StreamListEntry,
        StreamStart,
    },
    server::{
        ApiError,
        ServerState,
        cbor::Cbor,
        endpoints::AsCloseCode,
    },
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Multiplexer(#[from] crate::multiplexer::Error),

    #[error(transparent)]
    Axum(#[from] axum::Error),

    #[error(transparent)]
    CborDeserialize(#[from] ciborium::de::Error<std::io::Error>),

    #[error(transparent)]
    CborSerialize(#[from] ciborium::ser::Error<std::io::Error>),

    #[error("Unexpected empty frame")]
    UnexpectedEmptyFrame,

    #[error("Unexpected data frame")]
    UnexpectedData,

    #[error("Stream is closed: {stream_id}")]
    StreamClosed { stream_id: StreamId },
}

pub async fn create_stream(State(_state): State<ServerState>) -> impl IntoResponse {
    todo!();
}

pub async fn list_streams(State(state): State<ServerState>) -> Cbor<Vec<StreamListEntry>> {
    Cbor(
        state
            .multiplexer
            .list_streams()
            .iter()
            .map(|handle| {
                StreamListEntry {
                    id: handle.stream_id(),
                    info: handle.stream_info(),
                }
            })
            .collect(),
    )
}

pub async fn get_stream(
    State(state): State<ServerState>,
    Path(stream_id): Path<StreamId>,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    const PROTOCOL: &str = "mrrp-plex/stream";

    let stream_handle = state
        .multiplexer
        .get_stream(stream_id)
        .ok_or_else(|| ApiError::StreamNotFound { stream_id })?;

    let response = ws
        .protocols([PROTOCOL])
        .on_upgrade(async move |socket: WebSocket| {
            assert!(
                socket
                    .protocol()
                    .is_some_and(|protocol| protocol == PROTOCOL),
                "TODO: Reject websocket upgrade"
            );

            let mut connection = Connection::new(socket);

            let handler = async {
                let handler = ConnectionHandler::new(&mut connection, stream_handle).await?;
                handler.run().await
            };

            if let Err(error) = handler.await {
                tracing::error!(%error);
                connection.close_with_error(&error).await.ok();
            }
            else {
                connection.close().await.ok();
            }
        });

    Ok(response)
}

#[derive(Debug)]
struct ConnectionHandler<'a> {
    connection: &'a mut Connection,
    stream_handle: StreamHandle,
    frame_receiver: mpsc::Receiver<Bytes>,
}

impl<'a> ConnectionHandler<'a> {
    pub async fn new(
        connection: &'a mut Connection,
        stream_handle: StreamHandle,
    ) -> Result<Self, Error> {
        let (frame_sender, frame_receiver) = mpsc::channel(64);

        // todo: optional compression
        let sink = PcmWriter::<_, Complex<f32>, BigEndian>::new(FrameWriter::new(frame_sender));

        let stream = stream_handle.subscribe().map_err(|_| {
            Error::StreamClosed {
                stream_id: stream_handle.stream_id(),
            }
        })?;

        // todo: make buffer size adjustable. there's also some hard-coded
        // values in `FrameWriter` that should be adjustable and relative to
        // this.
        let _join_handle = tokio::spawn(async move {
            if let Err(error) = stream.forward(sink, 4096).await {
                match error {
                    // source error is !, so this variant is uninhabited
                    //
                    // ForwardError::Source(_) => unreachable!(),
                    ForwardError::Sink(error) => {
                        // this is a std::io::Error, but `FrameWriter` never
                        // returns an error
                        panic!("Unexpected IO error from FrameWriter: {error}");
                    }
                }
            }
        });

        // tell client that the stream started
        connection
            .send_message(&StreamStart {
                id: stream_handle.stream_id(),
                info: stream_handle.stream_info(),
            })
            .await?;

        Ok(Self {
            connection,
            stream_handle,
            frame_receiver,
        })
    }

    pub async fn run(mut self) -> Result<(), Error> {
        loop {
            tokio::select! {
                biased;
                result = self.connection.receive_message::<StreamCommand>() => {
                    let Some(command) = result? else { break; };
                    self.handle_command(command).await?;
                }
                result = self.stream_handle.wait_stream_info(|stream_info| Some(stream_info.clone())) => {
                    let Ok(info) = result else { break; };
                    self.connection.send_message(&StreamEvent::InfoChanged { info }).await?;
                }
                option = self.frame_receiver.recv() => {
                    let Some(frame) = option else { break; };
                    self.connection.socket.send(Message::Binary(frame)).await?;
                }
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: StreamCommand) -> Result<(), Error> {
        tracing::debug!(?command, "Received command");

        match command {
            StreamCommand::SetCenterFrequency { center_frequency } => {
                tracing::debug!(?center_frequency, "todo: set center frequency");
            }
            StreamCommand::SetSampleRate { sample_rate } => {
                tracing::debug!(?sample_rate, "todo: set sample rate");
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
enum ReceivedFrame<T> {
    Message(T),
    Data(Bytes),
}

const TAG_MESSAGE: u8 = b'm';
const TAG_DATA: u8 = b'd';

#[derive(Debug)]
struct Connection {
    socket: WebSocket,
}

impl Connection {
    #[inline]
    pub fn new(socket: WebSocket) -> Self {
        Self { socket }
    }

    #[inline]
    pub async fn receive<T>(&mut self) -> Result<Option<ReceivedFrame<T>>, Error>
    where
        T: DeserializeOwned,
    {
        loop {
            if let Some(message) = self.socket.recv().await.transpose()? {
                match message {
                    Message::Binary(mut bytes) => {
                        let frame = match bytes.try_get_u8() {
                            Ok(TAG_MESSAGE) => {
                                let message = ciborium::from_reader::<T, _>(bytes.reader())?;
                                ReceivedFrame::Message(message)
                            }
                            Ok(TAG_DATA) => ReceivedFrame::Data(bytes),
                            Ok(_tag) => {
                                // ignore frames with unknown tag
                                continue;
                            }
                            Err(_) => {
                                // empty frame
                                return Err(Error::UnexpectedEmptyFrame);
                            }
                        };

                        return Ok(Some(frame));
                    }
                    Message::Close(close_frame) => {
                        tracing::debug!(?close_frame, "client closing connection");
                        return Ok(None);
                    }
                    _ => {}
                }
            }
        }
    }

    #[inline]
    pub async fn receive_message<T>(&mut self) -> Result<Option<T>, Error>
    where
        T: DeserializeOwned,
    {
        match self.receive().await? {
            None => Ok(None),
            Some(ReceivedFrame::Data(_data)) => Err(Error::UnexpectedData),
            Some(ReceivedFrame::Message(message)) => Ok(Some(message)),
        }
    }

    pub async fn send_message<T>(&mut self, message: &T) -> Result<(), Error>
    where
        T: Serialize,
    {
        let mut data = BytesMut::new();

        data.put_u8(TAG_MESSAGE);
        ciborium::into_writer(message, (&mut data).writer())?;

        self.socket.send(Message::Binary(data.freeze())).await?;

        Ok(())
    }

    pub async fn close(mut self) -> Result<(), Error> {
        self.socket
            .send(Message::Close(Some(CloseFrame {
                code: 1000,
                reason: Default::default(),
            })))
            .await?;
        Ok(())
    }

    pub async fn close_with_error<E>(mut self, error: &E) -> Result<(), Error>
    where
        E: Display + AsCloseCode,
    {
        self.socket
            .send(Message::Close(Some(CloseFrame {
                code: error.as_close_code(),
                reason: error.to_string().into(),
            })))
            .await?;
        Ok(())
    }
}

impl AsCloseCode for Error {
    #[inline]
    fn as_close_code(&self) -> u16 {
        match self {
            Error::CborDeserialize(_) | Error::UnexpectedData | Error::UnexpectedEmptyFrame => {
                // Invalid Payload data
                1007
            }
            Error::StreamClosed { stream_id: _ } => {
                // Going away
                1001
            }
            _ => {
                // Internal error
                1011
            }
        }
    }
}

/// An async writer that splits the data into frames and prepends them with the
/// data tag [`TAG_DATA`].
#[derive(Debug)]
struct FrameWriter {
    frame_sender: PollSender<Bytes>,
    min_frame_size: NonZero<usize>,
    max_frame_size: NonZero<usize>,
    buffer: BytesMut,
}

impl FrameWriter {
    pub fn new(frame_sender: mpsc::Sender<Bytes>) -> Self {
        Self {
            frame_sender: PollSender::new(frame_sender),
            min_frame_size: const { NonZero::new(1024).unwrap() },
            max_frame_size: const { NonZero::new(4096).unwrap() },
            buffer: BytesMut::new(),
        }
    }

    fn flush_impl(&mut self, cx: &mut std::task::Context<'_>) -> Poll<Result<(), std::io::Error>> {
        let map_err = |_| std::io::Error::from(std::io::ErrorKind::BrokenPipe);

        ready!(self.frame_sender.poll_reserve(cx)).map_err(map_err)?;

        let frame = std::mem::take(&mut self.buffer).freeze();
        self.frame_sender.send_item(frame).map_err(map_err)?;

        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for FrameWriter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = &mut *self;

        if this.buffer.len() >= this.min_frame_size.get() {
            // we first need to flush this frame
            ready!(this.flush_impl(cx))?;
        }

        if this.buffer.is_empty() {
            // reserve enough space upfront
            this.buffer.reserve(this.max_frame_size.get());

            // write header with sink id
            this.buffer.put_u8(TAG_DATA);
        }

        // copy data into buffer
        assert!(this.buffer.len() < this.max_frame_size.get());
        let frame_remaining = this.max_frame_size.get() - this.buffer.len();
        let n = frame_remaining.min(buf.len());
        this.buffer.copy_from_slice(&buf[..n]);

        Poll::Ready(Ok(n))
    }

    #[inline]
    fn poll_flush(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        self.flush_impl(cx)
    }

    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<std::io::Result<()>> {
        ready!(self.flush_impl(cx))?;
        self.frame_sender.close();
        Poll::Ready(Ok(()))
    }
}
