use std::{
    num::NonZero,
    pin::Pin,
    task::{
        Context,
        Poll,
    },
};

use anyhow::{
    Error,
    anyhow,
};
use bytes::Buf;
use futures_util::TryStreamExt;
use mrrp_rtl_sdr::{
    Device,
    device::ReaderOptions,
    tuner::gain,
};
use mrrp_rtl_tcp::{
    protocol::TunerGainMode,
    server::{
        CommandReceiver,
        Connection,
        RawSink,
        Server,
        Sink,
    },
};
use mrrp_util::signal::broadcast::{
    self,
    NoSubscribers,
};
use tokio::{
    io::{
        AsyncBufReadExt,
        AsyncWrite,
    },
    net::TcpListener,
    sync::{
        mpsc,
        oneshot,
    },
};
use tokio_util::sync::CancellationToken;
use tracing::{
    Instrument,
    Span,
};

#[derive(Clone, Copy, Debug)]
pub struct ServerConfig {
    pub buffer_size: NonZero<usize>,
    pub fix_tuner_frequency: Option<f32>,
    pub log_dropped: bool,
}

pub async fn run(
    mut device: Device,
    listener: TcpListener,
    shutdown: CancellationToken,
    config: ServerConfig,
) -> Result<(), Error> {
    let server = Server::from_listener(listener);

    let tuner_type = tuner_type(&mut device).await;

    let tuner_gain_count = device.available_tuner_gains().len();
    let tuner_gain_count = tuner_gain_count.try_into().unwrap_or_else(|_| {
        panic!("The tuner gain count is too large for a u32: {tuner_gain_count}")
    });

    let dongle_info = mrrp_rtl_tcp::DongleInfo {
        tuner_type,
        tuner_gain_count,
    };
    tracing::debug!(?dongle_info);

    // spawn task handling commands for the rtl-sdr
    let (command_sender, command_receiver) = mpsc::channel(64);
    let _command_task = tokio::spawn(handle_commands(device, command_receiver, config));

    // spawn task reading samples from the rtl-sdr
    let (data_sender, data_subscriber) = broadcast::pipe(config.buffer_size);
    let _data_task = tokio::spawn({
        let shutdown = shutdown.clone();
        let command_sender = command_sender.clone();

        async move {
            if let Err(error) = handle_data(data_sender, command_sender.clone()).await {
                tracing::error!(%error, "Error in server data handler");
                shutdown.cancel();
            }
        }
    });

    loop {
        let accept = tokio::select! {
            result = server.wait() => result?,
            _ = shutdown.cancelled() => break,
        };

        let span = tracing::info_span!("connection", address = ?accept.address());
        let _guard = span.clone().entered();

        tracing::info!("New connection");

        let Connection {
            address: _,
            command_receiver,
            sink,
        } = accept.accept(dongle_info, Default::default()).await?;

        let _conn_command_task = tokio::spawn({
            let command_sender = command_sender.clone();

            async move {
                if let Err(error) =
                    handle_connection_commands(command_receiver, command_sender).await
                {
                    tracing::error!(?error, "Error in connection command handler");
                }

                tracing::info!("Connection closed");
            }
            .instrument(span.clone())
        });

        let Ok(data_receiver) = data_subscriber.subscribe()
        else {
            break;
        };

        let _conn_data_task = tokio::spawn(
            async move {
                if let Err(error) =
                    HandleConnectionData::new(sink, data_receiver, config.log_dropped).await
                {
                    tracing::error!(?error, "Error in connection data handler");
                }
            }
            .instrument(span),
        );
    }

    tracing::debug!("shutting down");
    let (result_sender, result_receiver) = oneshot::channel();
    command_sender
        .send(Command::Shutdown { result_sender })
        .await
        .map_err(|_| anyhow!("command handling task dead"))?;

    result_receiver
        .await
        .map_err(|_| anyhow!("command handling task didn't reply with result"))??;

    Ok(())
}

async fn handle_connection_commands(
    mut command_receiver: CommandReceiver,
    command_sender: mpsc::Sender<Command>,
) -> Result<(), Error> {
    loop {
        match command_receiver.try_next().await {
            Ok(None) => break,
            Err(mrrp_rtl_tcp::server::Error::Io(error))
                if error.kind() == std::io::ErrorKind::ConnectionReset =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
            Ok(Some(command)) => {
                let (result_sender, result_receiver) = oneshot::channel();
                command_sender
                    .send(Command::Client {
                        command,
                        result_sender,
                        span: Span::current(),
                    })
                    .await
                    .map_err(|_| anyhow!("command handling task dead"))?;

                result_receiver
                    .await
                    .map_err(|_| anyhow!("command handling task didn't reply with result"))??;
            }
        }
    }

    Ok(())
}

#[derive(Debug)]
struct HandleConnectionData {
    sink: RawSink,
    data_receiver: broadcast::Receiver<u8>,
    log_dropped: bool,
}

impl HandleConnectionData {
    #[inline]
    fn new(sink: Sink, data_receiver: broadcast::Receiver<u8>, log_dropped: bool) -> Self {
        Self {
            sink: sink.into_raw(),
            data_receiver,
            log_dropped,
        }
    }
}

impl Future for HandleConnectionData {
    type Output = Result<(), std::io::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        loop {
            match Pin::new(&mut this.data_receiver).poll_receive(cx) {
                Poll::Pending => return Poll::Pending,
                Poll::Ready(Err(broadcast::NoSenders)) => {
                    return Poll::Ready(Ok(()));
                }
                Poll::Ready(Ok(mut receive_guard)) => {
                    if this.log_dropped {
                        let num_dropped = receive_guard.num_dropped();
                        if num_dropped > 0 {
                            tracing::warn!(?num_dropped, "Dropped samples");
                        }
                    }

                    while receive_guard.has_remaining() {
                        let chunk = receive_guard.chunk();

                        match Pin::new(&mut this.sink).poll_write(cx, chunk) {
                            Poll::Pending => return Poll::Pending,
                            Poll::Ready(Err(error))
                                if error.kind() == std::io::ErrorKind::BrokenPipe =>
                            {
                                // socket closed
                                return Poll::Ready(Ok(()));
                            }
                            Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                            Poll::Ready(Ok(num_bytes_written)) => {
                                receive_guard.advance(num_bytes_written);
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn tuner_type(device: &mut mrrp_rtl_sdr::Device) -> mrrp_rtl_tcp::TunerType {
    let mrrp_rtl_sdr::device::Inner { rtl2832u: _, tuner } = &*device.inner_mut().await;

    let tuner_type = if let Some(tuner) = tuner
        .downcast_ref::<mrrp_rtl_sdr::tuner::r82xx::R82xx>()
        .or_else(|| {
            tuner
                .downcast_ref::<mrrp_rtl_sdr::tuner::r82xx::blog::BlogTuner>()
                .map(|blog| &blog.r82xx)
        }) {
        match tuner.model() {
            mrrp_rtl_sdr::tuner::r82xx::Model::R820T => Some(mrrp_rtl_tcp::TunerType::R820T),
            mrrp_rtl_sdr::tuner::r82xx::Model::R828D => Some(mrrp_rtl_tcp::TunerType::R828D),
            _ => None,
        }
    }
    else {
        None
    };

    tuner_type.unwrap_or(mrrp_rtl_tcp::TunerType::UNKNOWN)
}

#[tracing::instrument(skip_all)]
async fn handle_commands(
    mut device: mrrp_rtl_sdr::Device,
    mut command_receiver: mpsc::Receiver<Command>,
    config: ServerConfig,
) {
    while let Some(command) = command_receiver.recv().await {
        match command {
            Command::Client {
                command,
                result_sender,
                span,
            } => {
                let result = handle_command(&mut device, command, &config)
                    .instrument(span)
                    .await;
                let _ = result_sender.send(result);
            }
            Command::GetReader { result_sender } => {
                let result = device
                    .reader(ReaderOptions {
                        buffer_size: config.buffer_size.get(),
                        ..Default::default()
                    })
                    .await
                    .map_err(Into::into);
                let _ = result_sender.send(result);
            }
            Command::Shutdown { result_sender } => {
                let result = device.close().await.map_err(Into::into);
                let _ = result_sender.send(result);

                break;
            }
        }
    }
}

async fn handle_command(
    device: &mut mrrp_rtl_sdr::Device,
    command: mrrp_rtl_tcp::protocol::Command,
    config: &ServerConfig,
) -> Result<(), Error> {
    use mrrp_rtl_tcp::protocol::Command;

    async fn set_tuner_gain(
        device: &mut mrrp_rtl_sdr::Device,
        gain: impl gain::IntoTunerGain,
    ) -> Result<(), Error> {
        device.set_tuner_gain(gain).await?;
        Ok(())
    }

    match command {
        Command::SetSampleRate { sample_rate } => {
            tracing::debug!(?command, "handling command");
            device.set_sample_rate(sample_rate as f32).await?;
        }
        Command::SetCenterFrequency { frequency } => {
            tracing::debug!(?command, "handling command");
            let frequency = frequency as f32;

            if let Some(fixed_tuner_frequency) = config.fix_tuner_frequency {
                device.set_center_frequency(fixed_tuner_frequency).await?;
                let if_offset = frequency - fixed_tuner_frequency;
                device.set_if_offset(if_offset).await?;
            }
            else {
                device.set_center_frequency(frequency).await?;
            }
        }
        Command::SetAgcMode { enable } => {
            tracing::debug!(?command, "handling command");
            device.set_agc_mode(enable).await?;
        }
        Command::SetTunerGainMode { mode } => {
            tracing::debug!(?command, "handling command");

            match mode {
                TunerGainMode::Manual => {
                    set_tuner_gain(device, gain::Index(0)).await?;
                }
                TunerGainMode::Auto => {
                    set_tuner_gain(device, gain::Auto).await?;
                }
            }
        }
        Command::SetTunerGain { gain } => {
            tracing::debug!(?command, "handling command");
            set_tuner_gain(device, gain::Db(gain as f32 * 0.1)).await?;
        }
        Command::SetTunerGainIndex { index } => {
            tracing::debug!(?command, "handling command");
            set_tuner_gain(device, gain::Index(index.try_into()?)).await?;
        }
        Command::SetBiasT { enable } => {
            tracing::debug!(?command, "handling command");
            device.set_bias_t(enable).await?;
        }
        _ => tracing::debug!(?command, "ignoring command"),
    }

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn handle_data(
    mut data_sender: broadcast::Sender<u8>,
    command_sender: mpsc::Sender<Command>,
) -> Result<(), Error> {
    let mut reader_opt: Option<mrrp_rtl_sdr::Reader> = None;

    loop {
        if let Some(reader) = &mut reader_opt {
            // we have a reader so we need to read the data from it into our
            // ring buffer

            // the underlying reader is an AsyncBufRead, so we can ask it to
            // receive more data if necessary, and then give us its
            // buffer.
            let data = reader.fill_buf().await?;

            // put that data into our ring buffer.
            // this returns Ok(()) if there are any receivers to receive that
            // data. if there are, the data will have been written to the
            // buffer. if there aren't, it doesn't really matter, since no
            // receiver will ever see that data.
            match data_sender.send(data) {
                Ok(()) => {
                    let n = data.len();
                    reader.consume(n);
                }
                Err(broadcast::NoReceivers) => {
                    tracing::debug!("no receivers left");
                    // no receivers left. close reader
                    if let Some(reader) = reader_opt.take() {
                        reader.close().await?;
                    }
                }
            }
        }
        else {
            // we don't have a reader, meaning we don't know of any receivers
            // yet. wait for some
            tracing::debug!("waiting for receivers...");
            if let Err(NoSubscribers) = data_sender.wait_for_receivers().await {
                // no receivers and no subscribers left. we're done here
                break;
            }

            // we have receivers now, so we need to get a reader
            let (result_sender, result_receiver) = oneshot::channel();
            if command_sender
                .send(Command::GetReader { result_sender })
                .await
                .is_err()
            {
                // command receiver closed
                break;
            }

            // receive reader from command task. if the result_sender is
            // dropped, we exit
            let Ok(reader) = result_receiver.await
            else {
                break;
            };
            let reader = reader?;

            tracing::debug!(?reader, "start reading");
            reader_opt = Some(reader);
        }
    }

    tracing::debug!("done");

    Ok(())
}

enum Command {
    Client {
        command: mrrp_rtl_tcp::protocol::Command,
        result_sender: oneshot::Sender<Result<(), Error>>,
        span: Span,
    },
    GetReader {
        result_sender: oneshot::Sender<Result<mrrp_rtl_sdr::Reader, Error>>,
    },
    Shutdown {
        result_sender: oneshot::Sender<Result<(), Error>>,
    },
}
