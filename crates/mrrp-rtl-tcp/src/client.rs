use std::{
    num::NonZero,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use futures_util::{
    SinkExt,
    TryStreamExt,
};
use mrrp_core::{
    sample::{
        Complex,
        encoding::BigEndian,
    },
    signal::{
        AsyncReadSamples,
        ReadBuf,
        Remaining,
        StreamLength,
    },
};
use mrrp_util::signal::pcm::PcmReader;
use tokio::{
    io::{
        AsyncReadExt,
        BufReader,
    },
    net::{
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
    DEFAULT_BUF_SIZE,
    DongleInfo,
    protocol::{
        Command,
        CommandCodec,
        DecoderError,
        HEADER_LENGTH,
        HeaderCodec,
        InvalidHeader,
    },
};

/// Client errors
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    InvalidHeader(#[from] InvalidHeader),
}

impl From<DecoderError<InvalidHeader>> for Error {
    #[inline]
    fn from(value: DecoderError<InvalidHeader>) -> Self {
        match value {
            DecoderError::Protocol(error) => Self::InvalidHeader(error),
            DecoderError::Io(error) => Self::Io(error),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ConnectOptions {
    pub read_buffer_capacity: NonZero<usize>,
    pub write_buffer_capacity: NonZero<usize>,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            read_buffer_capacity: DEFAULT_BUF_SIZE,
            write_buffer_capacity: DEFAULT_BUF_SIZE,
        }
    }
}

/// A client for `rtl_tcp`
#[derive(Debug)]
pub struct Client {
    pub dongle_info: DongleInfo,
    pub command_sender: CommandSender,
    pub source: Source,
}

impl Client {
    /// Connnect to a `rtl_tcp` server.
    ///
    /// This implements [`AsyncReadSamples`] for async reading of IQ samples,
    /// and [`Configure`] to configure the receiver.
    pub async fn connect<A: ToSocketAddrs>(
        address: A,
        options: ConnectOptions,
    ) -> Result<Self, Error> {
        let socket = TcpStream::connect(address).await?;
        let address = socket.peer_addr()?;

        let (reader, writer) = socket.into_split();
        let mut reader = BufReader::with_capacity(options.read_buffer_capacity.get(), reader);

        // read header
        //
        // note: we limit the reader to the header length. otherwise
        // `FramedRead` might read more and buffer it and we would need to pull
        // it out and then create an adapter `AsyncRead` that consumes the
        // left-over from `FramedRead` first. This is simpler.
        let mut header_reader = FramedRead::with_capacity(
            (&mut reader).take(HEADER_LENGTH.try_into().unwrap()),
            HeaderCodec,
            HEADER_LENGTH,
        );
        let header = header_reader
            .try_next()
            .await?
            .ok_or_else(|| Error::Io(std::io::ErrorKind::UnexpectedEof.into()))?;

        // note: we don't wrap this in a `BufWriter`, since the `FramedWrite`
        // already has a buffer.
        let command_writer =
            FramedWrite::with_capacity(writer, CommandCodec, options.write_buffer_capacity.get());

        let pcm_reader = PcmReader::new(reader);

        tracing::debug!(?address, dongle_info = ?header.dongle_info, "connected to server");

        Ok(Self {
            dongle_info: header.dongle_info,
            command_sender: CommandSender { command_writer },
            source: Source { pcm_reader },
        })
    }
}

/// Sends commands to the `rtl_tcp` server.
///
/// # TODO
///
/// Implement `set_tuner_gain`. What argument should this method take, or should
/// the method be split into set mode, set gain in tenths of a dB, and index.
#[derive(Debug)]
pub struct CommandSender {
    command_writer: FramedWrite<OwnedWriteHalf, CommandCodec>,
}

impl CommandSender {
    /// Sends a command to the server.
    pub async fn send_command(&mut self, command: Command) -> Result<(), Error> {
        tracing::debug!(?command, "sending command");
        self.command_writer.send(command).await?;
        Ok(())
    }

    #[inline]
    pub async fn set_center_frequency(&mut self, frequency: u32) -> Result<(), Error> {
        self.send_command(Command::SetCenterFrequency { frequency })
            .await
    }

    #[inline]
    pub async fn set_sample_rate(&mut self, sample_rate: u32) -> Result<(), Error> {
        self.send_command(Command::SetSampleRate { sample_rate })
            .await
    }

    #[inline]
    pub async fn set_agc_mode(&mut self, enable: bool) -> Result<(), Error> {
        self.send_command(Command::SetAgcMode { enable }).await
    }

    #[inline]
    pub async fn set_frequency_correction(&mut self, ppm: i32) -> Result<(), Error> {
        self.send_command(Command::SetFrequencyCorrection { ppm })
            .await
    }

    #[inline]
    pub async fn set_tuner_if_gain(&mut self, stage: i16, gain: i16) -> Result<(), Error> {
        self.send_command(Command::SetTunerIfGain { stage, gain })
            .await
    }

    #[inline]
    pub async fn set_offset_tuning(&mut self, enable: bool) -> Result<(), Error> {
        self.send_command(Command::SetOffsetTuning { enable }).await
    }

    #[inline]
    pub async fn set_rtl_xtal(&mut self, frequency: u32) -> Result<(), Error> {
        self.send_command(Command::SetRtlXtal { frequency }).await
    }

    #[inline]
    pub async fn set_tuner_xtal(&mut self, frequency: u32) -> Result<(), Error> {
        self.send_command(Command::SetTunerXtal { frequency }).await
    }

    #[inline]
    pub async fn set_bias_tee(&mut self, enable: bool) -> Result<(), Error> {
        self.send_command(Command::SetBiasT { enable }).await
    }
}

#[derive(Debug)]
pub struct Source {
    pcm_reader: PcmReader<BufReader<OwnedReadHalf>, Complex<u8>, BigEndian>,
}

impl AsyncReadSamples for Source {
    type Sample = Complex<u8>;
    type Error = std::io::Error;

    #[inline]
    fn poll_read_samples(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut ReadBuf<Self::Sample>,
    ) -> Poll<Result<(), Self::Error>> {
        Pin::new(&mut self.pcm_reader).poll_read_samples(cx, buffer)
    }
}

impl StreamLength for Source {
    #[inline]
    fn remaining(&self) -> Remaining {
        Remaining::Unknown
    }
}
