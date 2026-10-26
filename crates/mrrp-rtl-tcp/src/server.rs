use std::{
    net::SocketAddr,
    num::NonZero,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::{
    SinkExt,
    Stream,
    StreamExt,
};
use mrrp_core::{
    sample::Complex,
    signal::AsyncWriteSamples,
};
use mrrp_util::signal::pcm::sink::{
    PcmSink,
    RawWriterError,
};
use tokio::{
    io::{
        AsyncWrite,
        BufReader,
        BufWriter,
    },
    net::{
        TcpListener,
        TcpStream,
        ToSocketAddrs,
        tcp::{
            OwnedReadHalf,
            OwnedWriteHalf,
        },
    },
};
use tokio_util::codec::{
    FramedRead,
    FramedWrite,
};

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
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    InvalidCommand(#[from] InvalidCommand),
}

impl From<DecoderError<InvalidCommand>> for Error {
    fn from(value: DecoderError<InvalidCommand>) -> Self {
        match value {
            DecoderError::Protocol(error) => Self::InvalidCommand(error),
            DecoderError::Io(error) => Self::Io(error),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AcceptOptions {
    pub read_buffer_capacity: NonZero<usize>,
    pub write_buffer_capacity: NonZero<usize>,
}

/// Default read and write buffer size (8 kiB)
pub const DEFAULT_BUF_SIZE: NonZero<usize> = const { NonZero::new(0x2000).unwrap() };

impl Default for AcceptOptions {
    fn default() -> Self {
        Self {
            read_buffer_capacity: DEFAULT_BUF_SIZE,
            write_buffer_capacity: DEFAULT_BUF_SIZE,
        }
    }
}

#[derive(Debug)]
pub struct Server {
    listener: TcpListener,
}

impl Server {
    pub fn from_listener(listener: TcpListener) -> Self {
        Self { listener }
    }

    pub async fn bind<A: ToSocketAddrs>(address: A) -> Result<Self, Error> {
        let listener = TcpListener::bind(address).await?;
        Ok(Self::from_listener(listener))
    }

    pub async fn wait(&self) -> Result<Accept, Error> {
        let (socket, address) = self.listener.accept().await?;

        Ok(Accept { address, socket })
    }
}

#[derive(Debug)]
pub struct Accept {
    address: SocketAddr,
    socket: TcpStream,
}

impl Accept {
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub async fn accept(
        self,
        dongle_info: DongleInfo,
        options: AcceptOptions,
    ) -> Result<Connection, Error> {
        let (reader, writer) = self.socket.into_split();

        let mut writer = BufWriter::with_capacity(options.write_buffer_capacity.get(), writer);
        let reader = BufReader::with_capacity(options.read_buffer_capacity.get(), reader);

        // write header
        let mut header_writer = FramedWrite::new(&mut writer, HeaderCodec);
        header_writer.send(Header { dongle_info }).await?;
        header_writer.flush().await?;

        let command_reader = FramedRead::new(reader, CommandCodec);
        let pcm_sink = PcmSink::new(writer);

        Ok(Connection {
            address: self.address,
            commands: Commands {
                reader: command_reader,
            },
            sink: Sink { inner: pcm_sink },
        })
    }
}

#[derive(Debug)]
pub struct Connection {
    pub address: SocketAddr,
    pub commands: Commands,
    pub sink: Sink,
}

#[derive(Debug)]
pub struct Commands {
    reader: FramedRead<BufReader<OwnedReadHalf>, CommandCodec>,
}

impl Stream for Commands {
    type Item = Result<Command, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.reader.poll_next_unpin(cx).map_err(Into::into)
    }
}

#[derive(Debug)]
pub struct Sink {
    // note: the PcmSink itself impls `AsyncWriteSamples` for a bunch of sample
    // types, but we only allow `Complex<u8>`
    inner: PcmSink<BufWriter<OwnedWriteHalf>>,
}

impl Sink {
    #[inline]
    pub fn into_raw(self) -> RawSink {
        RawSink {
            inner: self.inner.into_inner(),
        }
    }
}

impl AsyncWriteSamples<Complex<u8>> for Sink {
    type Error = Error;

    #[inline]
    fn poll_write_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[Complex<u8>],
    ) -> Poll<Result<usize, Self::Error>> {
        // note: this ugly syntax is needed because rustc can't infer the sample
        // type. here it can, but not in the other methods, and we wanted to
        // keep it uniform
        <PcmSink<_> as AsyncWriteSamples<Complex<u8>>>::poll_write_samples(
            Pin::new(&mut self.inner),
            cx,
            buffer,
        )
        .map_err(map_sink_err)
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // note: this ugly syntax is needed because rustc can't infer the sample
        // type.
        <PcmSink<_> as AsyncWriteSamples<Complex<u8>>>::poll_flush(Pin::new(&mut self.inner), cx)
            .map_err(map_sink_err)
    }

    #[inline]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        // note: this ugly syntax is needed because rustc can't infer the sample
        // type.
        <PcmSink<_> as AsyncWriteSamples<Complex<u8>>>::poll_close(Pin::new(&mut self.inner), cx)
            .map_err(map_sink_err)
    }
}

#[inline]
fn map_sink_err(error: RawWriterError<!>) -> Error {
    match error {
        RawWriterError::Writer(error) => Error::Io(error),
        // note: the other variant is uninhabited
    }
}

#[derive(Debug)]
pub struct RawSink {
    inner: BufWriter<OwnedWriteHalf>,
}

impl AsyncWrite for RawSink {
    #[inline]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}
