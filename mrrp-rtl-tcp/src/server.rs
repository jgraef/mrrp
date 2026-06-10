use std::{
    fmt::Debug,
    net::SocketAddr,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use bytes::Buf;
use futures_util::{
    SinkExt,
    TryStreamExt,
};
use pin_project_lite::pin_project;
use tokio::{
    io::{
        AsyncWrite,
        AsyncWriteExt,
        BufReader,
        BufWriter,
    },
    net::{
        TcpListener,
        TcpStream,
    },
};
use tokio_util::{
    codec::{
        FramedRead,
        FramedWrite,
    },
    sync::CancellationToken,
};
use tracing::Instrument;

use crate::{
    DongleInfo,
    protocol::{
        Command,
        CommandCodec,
        DecoderError,
        Header,
        HeaderCodec,
        InvalidCommand,
    },
};

/// Server errors
#[derive(Debug, thiserror::Error)]
pub enum Error<H> {
    #[error(transparent)]
    Socket(#[from] std::io::Error),

    /// Error from the underlying stream, e.g. the rtlsdr device, or another
    /// `rtl_tcp`` client.
    #[error(transparent)]
    Handler(H),

    #[error(transparent)]
    InvalidCommand(#[from] InvalidCommand),
}

impl<H> From<DecoderError<InvalidCommand>> for Error<H> {
    fn from(value: DecoderError<InvalidCommand>) -> Self {
        match value {
            DecoderError::Protocol(error) => Self::InvalidCommand(error),
            DecoderError::Io(error) => Self::Socket(error),
        }
    }
}

/// A `rtl_tcp` server.
///
/// Different from the original `rtl_tcp` this can accept multiple connections
/// at once.
pub struct RtlTcpServer<H> {
    handler: H,
    tcp_listener: TcpListener,
    cancellation_token: CancellationToken,
}

impl<H> RtlTcpServer<H> {
    /// Create a `rtl_tcp` server.
    ///
    /// The provided handler accepts clients by returning:
    ///
    /// - [`CommandHandler`]: Handles commands (e.g. set sample rate)
    /// - [`SampleStream`]: Streams the IQ data as bytes
    /// - [`DongleInfo`]: The initial information about the RTL-SDR that is sent
    ///   to the client.
    pub fn new(handler: impl IntoHandler<Handler = H>, tcp_listener: TcpListener) -> Self {
        Self {
            handler: handler.into_handler(),
            tcp_listener,
            cancellation_token: CancellationToken::new(),
        }
    }

    pub fn with_graceful_shutdown(mut self, cancellation_token: CancellationToken) -> Self {
        self.cancellation_token = cancellation_token;
        self
    }
}

impl<H> RtlTcpServer<H>
where
    H: Handler,
    H::CommandHandler: Send + 'static,
    H::SampleStream: Send + 'static,
{
    /// Serve incoming connections
    pub async fn serve(mut self) -> Result<(), Error<H::Error>> {
        tracing::debug!("waiting for connections");

        loop {
            tokio::select! {
                _ = self.cancellation_token.cancelled() => {
                    break;
                }
                result = self.tcp_listener.accept() => {
                    let (connection, address) = result?;
                    self.handle_accept(connection, address, self.cancellation_token.clone()).await?;
                }
            }
        }

        self.handler.shutdown().await.map_err(Error::Handler)?;

        Ok(())
    }

    async fn handle_accept(
        &mut self,
        connection: TcpStream,
        address: SocketAddr,
        cancellation_token: CancellationToken,
    ) -> Result<(), Error<H::Error>> {
        let span = tracing::info_span!("connection", %address);

        let (command_handler, sample_stream, dongle_info) = self
            .handler
            .accept_connection(address)
            .await
            .map_err(Error::Handler)?;

        tokio::spawn(
            async move {
                tracing::debug!(%address, "new connection");
                if let Err(error) = serve_connection(
                    connection,
                    command_handler,
                    sample_stream,
                    dongle_info,
                    cancellation_token,
                )
                .await
                {
                    tracing::error!(?error);
                }
                tracing::debug!(%address, "connection closed");
            }
            .instrument(span),
        );

        Ok(())
    }
}

pin_project! {
    #[derive(Debug)]
    struct ForwardDataFuture<S, W> {
        #[pin]
        sample_stream: S,
        #[pin]
        tcp_write: W,
    }
}

impl<S, W> Future for ForwardDataFuture<S, W>
where
    S: SampleStream,
    W: AsyncWrite,
{
    type Output = Result<(), Error<S::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let mut this = self.as_mut().project();

            match this.sample_stream.poll_fill_buffer(cx) {
                Poll::Pending => {
                    return Poll::Pending;
                }
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(Err(Error::Handler(error)));
                }
                Poll::Ready(Ok(mut buffer)) => {
                    if !buffer.has_remaining() {
                        // sample stream EOF
                        return Poll::Ready(Ok(()));
                    }

                    // get chunks from buffer and write them to the socket
                    loop {
                        let chunk = buffer.chunk();

                        if chunk.is_empty() {
                            // sample stream buffer exhausted.
                            break;
                        }

                        match this.tcp_write.as_mut().poll_write(cx, chunk) {
                            Poll::Pending => {
                                return Poll::Pending;
                            }
                            Poll::Ready(Err(error)) => {
                                return Poll::Ready(Err(Error::Socket(error)));
                            }
                            Poll::Ready(Ok(num_written)) => {
                                if num_written == 0 {
                                    // socket closed
                                    return Poll::Ready(Ok(()));
                                }

                                buffer.advance(num_written);
                            }
                        }
                    }
                }
            }
        }
    }
}

pub async fn serve_connection<C, S>(
    mut connection: TcpStream,
    mut command_handler: C,
    sample_stream: S,
    dongle_info: DongleInfo,
    cancellation_token: CancellationToken,
) -> Result<(), Error<C::Error>>
where
    C: CommandHandler,
    S: SampleStream<Error = C::Error>,
{
    // split connection into read and write part, and buffer it
    let (tcp_read, tcp_write) = connection.split();
    let tcp_read = BufReader::new(tcp_read);
    let mut tcp_write = BufWriter::new(tcp_write);

    // write header
    let mut header_write = FramedWrite::new(&mut tcp_write, HeaderCodec);
    header_write.send(Header { dongle_info }).await?;
    header_write.flush().await?;

    // this reads commands and calls the handler method with them
    let command_future = async move {
        let mut command_read = FramedRead::new(tcp_read, CommandCodec);

        while let Some(command) = command_read.try_next().await? {
            command_handler
                .handle_command(command)
                .await
                .map_err(Error::Handler)?
        }
        Ok::<(), Error<C::Error>>(())
    };

    // this forwards the sample from the handler to the TCP connection
    let data_future = ForwardDataFuture {
        sample_stream,
        tcp_write,
    };

    tokio::select! {
        _ = cancellation_token.cancelled() => {},
        result = command_future => result?,
        result = data_future => result?,
    }

    // shutdown the connection properly
    connection.shutdown().await?;

    Ok(())
}

pub trait Handler {
    type Error: Debug + Send;
    type CommandHandler: CommandHandler<Error = Self::Error>;
    type SampleStream: SampleStream<Error = Self::Error>;

    fn accept_connection(
        &mut self,
        address: SocketAddr,
    ) -> impl Future<Output = Result<(Self::CommandHandler, Self::SampleStream, DongleInfo), Self::Error>>;

    fn shutdown(&mut self) -> impl Future<Output = Result<(), Self::Error>>;
}

pub trait CommandHandler {
    type Error: Debug;

    fn handle_command(
        &mut self,
        command: Command,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

/// # TODO
///
/// Replace this with trait from mrrp-core
pub trait SampleStream {
    type Error: Debug;

    fn poll_fill_buffer<'a>(
        self: Pin<&'a mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<impl Buf + 'a, Self::Error>>;
}

pub trait IntoHandler {
    type Handler: Handler;

    fn into_handler(self) -> Self::Handler;
}

impl<H> IntoHandler for H
where
    H: Handler,
{
    type Handler = H;

    fn into_handler(self) -> H {
        self
    }
}
