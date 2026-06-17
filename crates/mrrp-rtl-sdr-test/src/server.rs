use std::{
    net::SocketAddr,
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
use mrrp_rtl_sdr::{
    device::ReaderOptions,
    tuner::gain,
};
use mrrp_rtl_tcp::{
    protocol::TunerGainMode,
    server,
};
use pin_project_lite::pin_project;
use tokio::{
    io::AsyncBufReadExt,
    sync::{
        mpsc,
        oneshot,
    },
};

use crate::server::ring_buffer::Closed;

#[derive(Clone, Copy, Debug)]
pub struct ServerConfig {
    pub buffer_size: usize,
    pub fix_tuner_frequency: Option<f32>,
}

#[derive(Debug)]
pub struct ServerHandler {
    command_sender: mpsc::Sender<Command>,
    data_subscriber: ring_buffer::Subscriber<u8>,
    dongle_info: mrrp_rtl_tcp::DongleInfo,
    log_dropped: bool,
}

impl ServerHandler {
    pub async fn new(
        mut device: mrrp_rtl_sdr::Device,
        config: ServerConfig,
    ) -> Result<Self, Error> {
        assert_ne!(config.buffer_size, 0, "buffer_size can't be 0");
        assert_eq!(
            config.buffer_size % 2,
            0,
            "buffer_size must be a multiple of 2"
        );

        let tuner_type = tuner_type(&mut device).await;

        let tuner_gain_count = device.available_tuner_gains().len();
        let tuner_gain_count = tuner_gain_count.try_into().unwrap_or_else(|_| {
            panic!("The tuner gain count is too large for a u32: {tuner_gain_count}")
        });

        let dongle_info = mrrp_rtl_tcp::DongleInfo {
            tuner_type,
            tuner_gain_count,
        };

        let (command_sender, command_receiver) = mpsc::channel(64);
        let (data_sender, data_subscriber) = ring_buffer::channel(config.buffer_size);

        let _command_task = tokio::spawn(handle_commands(device, command_receiver, config));
        let _data_task = tokio::spawn({
            let command_sender = command_sender.clone();

            async move {
                if let Err(error) = handle_data(data_sender, command_sender.clone()).await {
                    // todo: we need to propagate the error to the actual server
                    tracing::error!(%error, "server data handler error");
                    let _ = command_sender.send(Command::Shutdown {
                        result_sender: None,
                    });
                }
            }
        });

        Ok(Self {
            command_sender,
            data_subscriber,
            dongle_info,
            log_dropped: false,
        })
    }

    pub fn with_log_dropped(mut self, log_dropped: bool) -> Self {
        self.log_dropped = log_dropped;
        self
    }
}

async fn tuner_type(device: &mut mrrp_rtl_sdr::Device) -> mrrp_rtl_tcp::TunerType {
    let mrrp_rtl_sdr::device::Inner { rtl2832u: _, tuner } = &*device.inner_mut().await;

    if let Some(r82xx) = tuner.downcast_ref::<mrrp_rtl_sdr::tuner::r82xx::R82xx>() {
        match r82xx.model() {
            mrrp_rtl_sdr::tuner::r82xx::Model::R820T => return mrrp_rtl_tcp::TunerType::R820T,
            mrrp_rtl_sdr::tuner::r82xx::Model::R828D => return mrrp_rtl_tcp::TunerType::R828D,
            _ => {}
        }
    }

    mrrp_rtl_tcp::TunerType::UNKNOWN
}

impl server::Handler for ServerHandler {
    type Error = Error;
    type CommandHandler = CommandHandler;
    type SampleStream = SampleStream;

    async fn accept_connection(
        &mut self,
        _address: SocketAddr,
    ) -> Result<(CommandHandler, SampleStream, mrrp_rtl_tcp::DongleInfo), Self::Error> {
        let command_handler = CommandHandler {
            command_sender: self.command_sender.clone(),
        };

        let sample_stream = SampleStream {
            data_receiver: self.data_subscriber.subscribe(),
            log_dropped: self.log_dropped,
        };

        Ok((command_handler, sample_stream, self.dongle_info))
    }

    async fn shutdown(&mut self) -> Result<(), Self::Error> {
        tracing::info!("Shutting down server");
        let (result_sender, result_receiver) = oneshot::channel();
        let _ = self
            .command_sender
            .send(Command::Shutdown {
                result_sender: Some(result_sender),
            })
            .await;
        result_receiver.await.unwrap_or(Ok(()))
    }
}

#[derive(Debug)]
pub struct CommandHandler {
    command_sender: mpsc::Sender<Command>,
}

impl server::CommandHandler for CommandHandler {
    type Error = Error;

    async fn handle_command(
        &mut self,
        command: mrrp_rtl_tcp::protocol::Command,
    ) -> Result<(), Self::Error> {
        let (result_sender, result_receiver) = oneshot::channel();

        self.command_sender
            .send(Command::Client {
                command,
                result_sender,
            })
            .await
            .map_err(|_| anyhow!("reactor dead"))?;

        result_receiver.await?
    }
}

pin_project! {
    #[derive(Debug)]
    pub struct SampleStream {
        #[pin]
        data_receiver: ring_buffer::Receiver<u8>,
        log_dropped: bool,
    }
}

impl server::SampleStream for SampleStream {
    type Error = Error;

    fn poll_fill_buffer<'a>(
        self: Pin<&'a mut Self>,
        cx: &mut Context,
    ) -> Poll<Result<impl Buf + 'a, Self::Error>> {
        let this = self.project();

        match this.data_receiver.poll_receive(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(Err(Closed)) => Poll::Ready(Ok(Buffer::Eof)),
            Poll::Ready(Ok(receive_guard)) => {
                if *this.log_dropped {
                    let num_dropped = receive_guard.num_dropped();
                    if num_dropped > 0 {
                        tracing::warn!(?num_dropped, "Dropped samples");
                    }
                }

                Poll::Ready(Ok(Buffer::Filled(receive_guard)))
            }
        }
    }
}

#[derive(Debug)]
pub enum Buffer<'a> {
    Eof,
    Filled(ring_buffer::ReceiveGuard<'a, u8>),
}

impl<'a> Buf for Buffer<'a> {
    fn remaining(&self) -> usize {
        match self {
            Buffer::Eof => 0,
            Buffer::Filled(receive_guard) => receive_guard.remaining(),
        }
    }

    fn chunk(&self) -> &[u8] {
        match self {
            Buffer::Eof => &[],
            Buffer::Filled(receive_guard) => receive_guard.chunk(),
        }
    }

    fn advance(&mut self, cnt: usize) {
        match self {
            Buffer::Eof => {}
            Buffer::Filled(receive_guard) => receive_guard.advance(cnt),
        }
    }
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
            } => {
                let result = handle_command(&mut device, command, &config).await;
                let _ = result_sender.send(result);
            }
            Command::GetReader { result_sender } => {
                let result = device
                    .reader(ReaderOptions {
                        buffer_size: config.buffer_size,
                        ..Default::default()
                    })
                    .await
                    .map_err(Into::into);
                let _ = result_sender.send(result);
            }
            Command::Shutdown { result_sender } => {
                let result = device.close().await.map_err(Into::into);

                if let Some(result_sender) = result_sender {
                    let _ = result_sender.send(result);
                }

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
        _ => tracing::debug!(?command, "ignoring command"),
    }

    Ok(())
}

#[tracing::instrument(skip_all)]
async fn handle_data(
    mut data_sender: ring_buffer::Sender<u8>,
    command_sender: mpsc::Sender<Command>,
) -> Result<(), Error> {
    let mut reader_opt: Option<mrrp_rtl_sdr::Reader> = None;

    loop {
        if let Some(reader) = &mut reader_opt {
            // we have a reader so we need to read the data from it into our ring buffer

            // the underlying reader is an AsyncBufRead, so we can ask it to receive more
            // data if necessary, and then give us its buffer.
            let data = reader.fill_buf().await?;

            // put that data into our ring buffer.
            // this returns if there are any receivers to receive that data.
            // if there are, the data will have been written to the buffer.
            // if there aren't, it doesn't really matter, since no receiver will ever see
            // that data.
            if data_sender.send(data) {
                let n = data.len();
                reader.consume(n);
            }
            else {
                tracing::debug!("no receivers left");
                // no receivers left. drop reader
                if let Some(reader) = reader_opt.take() {
                    reader.close().await?;
                }
            }
        }
        else {
            // we don't have a reader, meaning we don't know of any receivers yet. wait for
            // some
            tracing::debug!("waiting for receivers...");
            if data_sender.wait_for_receivers().await.is_err() {
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

            // receive reader from command task. if the result_sender is dropped, we exit
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
    },
    GetReader {
        result_sender: oneshot::Sender<Result<mrrp_rtl_sdr::Reader, Error>>,
    },
    Shutdown {
        result_sender: Option<oneshot::Sender<Result<(), Error>>>,
    },
}

mod ring_buffer {
    #![allow(dead_code)]
    // todo: this would be super useful in mrrp (with Buf/BufMut impls)

    use std::{
        collections::VecDeque,
        pin::Pin,
        sync::Arc,
        task::{
            Context,
            Poll,
            Waker,
        },
    };

    use bytes::Buf;
    use parking_lot::{
        RwLock,
        RwLockReadGuard,
        RwLockWriteGuard,
    };
    use pin_project_lite::pin_project;

    /// The channel is closed and can't ever become open again.
    #[derive(Debug)]
    pub struct Closed;

    pub fn channel<T>(buffer_size: usize) -> (Sender<T>, Subscriber<T>) {
        let shared = Arc::new(Shared {
            state: RwLock::new(State {
                buffer: VecDeque::with_capacity(buffer_size),
                head_pos: 0,
                receiver_slots: vec![],
                sender_slot: Some(Slot { waker: None }),
                receiver_count: 0,
                subscriber_count: 1,
                sender_count: 1,
            }),
        });

        (
            Sender {
                shared: shared.clone(),
            },
            Subscriber { shared },
        )
    }

    #[derive(Debug)]
    struct Shared<T> {
        state: RwLock<State<T>>,
    }

    #[derive(Debug)]
    struct State<T> {
        buffer: VecDeque<T>,
        head_pos: usize,

        receiver_slots: Vec<Option<Slot>>,
        sender_slot: Option<Slot>,

        receiver_count: usize,
        subscriber_count: usize,
        sender_count: usize,
    }

    impl<T> State<T> {
        fn wake_all_receivers(&mut self) {
            // todo: do we need to clear the waker?
            self.receiver_slots
                .iter()
                .flatten()
                .flat_map(|slot| &slot.waker)
                .for_each(|waker| waker.wake_by_ref());
        }

        fn subscribe(&mut self) -> usize {
            // increase receiver count
            self.receiver_count += 1;

            // notify sender that there are receivers
            if let Some(slot) = &self.sender_slot
                && let Some(waker) = &slot.waker
            {
                waker.wake_by_ref();
            }

            // insert a receiver slot
            for (i, slot) in self.receiver_slots.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(Slot { waker: None });
                    return i;
                }
            }

            let i = self.receiver_slots.len();
            self.receiver_slots.push(Some(Slot { waker: None }));
            i
        }
    }

    #[derive(Debug)]
    struct Slot {
        waker: Option<Waker>,
    }

    pin_project! {
        #[derive(Debug)]
        pub struct Receiver<T> {
            inner: ReceiverInner<T>
        }
    }

    #[derive(Debug)]
    struct ReceiverInner<T> {
        shared: Arc<Shared<T>>,
        slot: usize,
        read_pos: usize,
    }

    impl<T> Receiver<T> {
        pub fn poll_receive<'a>(
            self: Pin<&'a mut Self>,
            cx: &mut Context,
        ) -> Poll<Result<ReceiveGuard<'a, T>, Closed>> {
            let this = self.project();

            // we do this in 2 steps
            //
            // 1: check if there's data and if so return it. this only needs a read guard
            // 2: register waker. this needs a write guard
            //
            // between 1 and 2 we don't hold a lock, so we need to rerun the checks in 2.
            // but 1 can be run by all readers in parallel. we hope this is more efficient
            // (benchmark lol!)

            // first we try to read through a read guard
            let new_waker = {
                let state_guard = this.inner.shared.state.read();

                if this.inner.read_pos < state_guard.head_pos {
                    // there's data :)
                    return Poll::Ready(Ok(ReceiveGuard {
                        state_guard,
                        read_pos: &mut this.inner.read_pos,
                    }));
                }

                // check if there's still a sender
                if state_guard.sender_count == 0 {
                    return Poll::Ready(Err(Closed));
                }

                // check if we need to insert our waker
                let slot = state_guard.receiver_slots[this.inner.slot]
                    .as_ref()
                    .unwrap();

                let new_waker = cx.waker();

                if let Some(waker) = &slot.waker
                    && new_waker.will_wake(waker)
                {
                    None
                }
                else {
                    Some(new_waker.clone())
                }
            };

            // at this point we don't hold the lock, so stuff could be written, or the
            // sender drop

            if let Some(waker) = new_waker {
                // we'll need a write guard to insert our waker

                let mut state_guard = this.inner.shared.state.write();

                // but in the meantime some data might have been written to the ring buffer
                if this.inner.read_pos < state_guard.head_pos {
                    return Poll::Ready(Ok(ReceiveGuard {
                        state_guard: RwLockWriteGuard::downgrade(state_guard),
                        read_pos: &mut this.inner.read_pos,
                    }));
                }

                // check if there's still a sender (again)
                if state_guard.sender_count == 0 {
                    return Poll::Ready(Err(Closed));
                }

                let slot = state_guard.receiver_slots[this.inner.slot]
                    .as_mut()
                    .unwrap();

                slot.waker = Some(waker);
            }

            Poll::Pending
        }

        pub fn subscriber(&self) -> Subscriber<T> {
            Subscriber {
                shared: self.inner.shared.clone(),
            }
        }
    }

    impl<T> Clone for ReceiverInner<T> {
        fn clone(&self) -> Self {
            let mut state_guard = self.shared.state.write();
            let slot = state_guard.subscribe();
            Self {
                shared: self.shared.clone(),
                slot,
                read_pos: self.read_pos,
            }
        }
    }

    impl<T> Drop for ReceiverInner<T> {
        fn drop(&mut self) {
            let mut state_guard = self.shared.state.write();
            state_guard.receiver_count -= 1;
            state_guard.receiver_slots[self.slot] = None;
        }
    }

    /// # TODO
    ///
    /// Implement `mrrp::Buf` on this
    #[derive(Debug)]
    pub struct ReceiveGuard<'a, T> {
        state_guard: RwLockReadGuard<'a, State<T>>,
        read_pos: &'a mut usize,
    }

    impl<'a, T> ReceiveGuard<'a, T> {
        #[allow(dead_code)]
        pub fn num_dropped(&self) -> usize {
            (self.state_guard.head_pos - *self.read_pos)
                .saturating_sub(self.state_guard.buffer.len())
        }
    }

    impl<'a> Buf for ReceiveGuard<'a, u8> {
        fn remaining(&self) -> usize {
            self.state_guard.head_pos - *self.read_pos
        }

        fn chunk(&self) -> &[u8] {
            let mut start_index = self
                .state_guard
                .buffer
                .len()
                .saturating_sub(self.state_guard.head_pos - *self.read_pos);

            let (tail, head) = self.state_guard.buffer.as_slices();

            let chunk = if start_index < tail.len() {
                tail
            }
            else {
                start_index -= tail.len();
                head
            };

            &chunk[start_index..]
        }

        fn advance(&mut self, cnt: usize) {
            let read_pos = *self.read_pos + cnt;
            assert!(
                read_pos <= self.state_guard.head_pos,
                "Called advance with cnt={cnt}, but only {} remaining",
                self.remaining()
            );

            *self.read_pos = read_pos;
        }
    }

    #[derive(Debug)]
    pub struct Sender<T> {
        shared: Arc<Shared<T>>,
    }

    impl<T> Sender<T>
    where
        T: Clone,
    {
        pub fn send(&mut self, data: &[T]) -> bool {
            if data.is_empty() {
                return true;
            }

            let mut state_guard = self.shared.state.write();

            if state_guard.receiver_count == 0 {
                // no receivers are here to ever observe the bytes we would write, so we don't.

                // technically, since there are no receivers that keep track of this, we
                // don't need to increase this
                state_guard.head_pos += data.len();

                false
            }
            else {
                // skip anything that would not end up in the buffer anyway
                let skip = data.len().saturating_sub(state_guard.buffer.capacity());

                // truncate to not exceed capacity
                let truncate = (data.len() + state_guard.buffer.len())
                    .saturating_sub(state_guard.buffer.capacity());
                let keep = state_guard.buffer.capacity().saturating_sub(truncate);
                debug_assert!(skip == 0 || keep != 0);
                state_guard.buffer.truncate_front(keep);

                // write data to buffer
                state_guard.buffer.extend(data.iter().cloned());

                // update head position
                state_guard.head_pos += data.len();

                // notify receivers
                state_guard.wake_all_receivers();

                true
            }
        }

        pub fn poll_wait_receivers(
            self: Pin<&mut Self>,
            cx: &mut Context,
        ) -> Poll<Result<(), Closed>> {
            let mut state_guard = self.shared.state.write();

            if state_guard.receiver_count > 0 {
                Poll::Ready(Ok(()))
            }
            else if state_guard.sender_count == 0 {
                Poll::Ready(Err(Closed))
            }
            else {
                let new_waker = cx.waker();
                let slot = state_guard.sender_slot.as_mut().unwrap();
                if let Some(waker) = &mut slot.waker {
                    waker.clone_from(new_waker);
                }
                else {
                    slot.waker = Some(new_waker.clone());
                }

                Poll::Pending
            }
        }

        pub async fn wait_for_receivers(&mut self) -> Result<(), Closed> {
            std::future::poll_fn(move |cx| Pin::new(&mut *self).poll_wait_receivers(cx)).await
        }
    }

    impl<T> Drop for Sender<T> {
        fn drop(&mut self) {
            let mut state_guard = self.shared.state.write();
            state_guard.sender_count -= 1;
            state_guard.sender_slot = None;

            state_guard.wake_all_receivers();
        }
    }

    #[derive(Debug)]
    pub struct Subscriber<T> {
        shared: Arc<Shared<T>>,
    }

    impl<T> Subscriber<T> {
        pub fn subscribe(&self) -> Receiver<T> {
            let mut state_guard = self.shared.state.write();

            Receiver {
                inner: ReceiverInner {
                    shared: self.shared.clone(),
                    slot: state_guard.subscribe(),
                    read_pos: state_guard.head_pos,
                },
            }
        }
    }

    impl<T> Clone for Subscriber<T> {
        fn clone(&self) -> Self {
            let mut state_guard = self.shared.state.write();
            state_guard.subscriber_count += 1;

            Self {
                shared: self.shared.clone(),
            }
        }
    }

    impl<T> Drop for Subscriber<T> {
        fn drop(&mut self) {
            let mut state_guard = self.shared.state.write();
            state_guard.subscriber_count -= 1;
        }
    }
}
