use std::{
    collections::VecDeque,
    sync::Arc,
};

use num_complex::Complex;
use tokio::sync::{
    broadcast,
    mpsc,
    oneshot,
    watch,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    RtlSdr(#[from] mrrp_rtl_sdr::Error),
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    pub chunk_size: usize,
    pub command_channel_capacity: usize,
    pub data_channel_capacity: usize,
    pub event_channel_capacity: usize,
}

#[derive(Debug)]
pub struct RtlSdr {
    command_sender: mpsc::Sender<Command>,

    /// Not actually for sending, but can be subscribed to if the user wants to
    /// start receiving data. While there are no receivers no data will be read
    /// from the rtl-sdr
    data_sender: watch::Sender<RingBuffer>,

    info: Arc<Info>,
}

impl RtlSdr {
    async fn new(device: mrrp_rtl_sdr::Device, options: Options) -> Result<Self, Error> {
        let (command_sender, command_receiver) = mpsc::channel(options.command_channel_capacity);

        let (data_sender, _) = watch::channel(RingBuffer {
            head_position: 0,
            buffer: VecDeque::new(),
        });

        // todo: fill these
        let info = Arc::new(Info {});
        let state = State {
            center_frequency: 7000000,
            frequency_correction: 0,
            sample_rate: 2400000,
            tuner_gain: TunerGain::Auto,
            bias_tee: false,
        };

        let reactor = Reactor::new(
            device,
            options,
            command_receiver,
            data_sender.clone(),
            state,
            info.clone(),
        );

        let _join_handle = tokio::spawn(reactor.run());

        Ok(Self {
            command_sender,
            data_sender,
            info,
        })
    }
}

#[derive(Debug)]
struct RingBuffer {
    head_position: usize,
    buffer: VecDeque<Chunk>,
}

#[derive(Debug)]
struct Chunk {
    data: Vec<Complex<u8>>,
}

struct Reactor {
    device: mrrp_rtl_sdr::Device,
    options: Options,
    command_receiver: mpsc::Receiver<Command>,
    data_sender: watch::Sender<RingBuffer>,
    event_sender: broadcast::Sender<Event>,
    state_sender: watch::Sender<State>,
    info: Arc<Info>,
}

impl Reactor {
    fn new(
        device: mrrp_rtl_sdr::Device,
        options: Options,
        command_receiver: mpsc::Receiver<Command>,
        data_sender: watch::Sender<RingBuffer>,
        state: State,
        info: Arc<Info>,
    ) -> Self {
        let (event_sender, _) = broadcast::channel(options.event_channel_capacity);

        let (state_sender, _) = watch::channel(state);

        Reactor {
            device,
            options,
            command_receiver,
            data_sender,
            event_sender,
            state_sender,
            info,
        }
    }

    async fn run(mut self) {
        // todo

        /*loop {
            self.handle_commands();

            self.handle_data();
        }*/
    }

    fn handle_commands(&mut self) {
        // handle all commands
        loop {
            if self.data_sender.is_closed() {
                // we don't have any data receivers, so we block waiting for commands
                if let Some(command) = self.command_receiver.blocking_recv() {
                    self.handle_command(command);
                }
                else {
                    // no more command senders. shutdown
                    return;
                }
            }
            else {
                // there are data receivers, so we can't block while reading commands.
                match self.command_receiver.try_recv() {
                    Ok(command) => {
                        self.handle_command(command);
                    }
                    Err(mpsc::error::TryRecvError::Empty) => {
                        // no more commands, handle data stream next
                        break;
                    }
                    Err(mpsc::error::TryRecvError::Disconnected) => {
                        // no more command senders. shutdown
                        return;
                    }
                }
            };
        }
    }

    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Events { result_sender } => {
                let state = self.state_sender.borrow().clone();
                let event_receiver = self.event_sender.subscribe();
                let _ = result_sender.send((state, event_receiver));
            }
            Command::State { result_sender } => {
                let state_receiver = self.state_sender.subscribe();
                let _ = result_sender.send(state_receiver);
            }
            Command::SetCenterFrequency {
                result_sender,
                center_frequency,
            } => {
                self.set_device_helper(
                    center_frequency,
                    result_sender,
                    |state| &mut state.center_frequency,
                    |_, _| {
                        tracing::warn!("todo: set center frequency");
                        Ok(())
                    },
                    Event::CenterFrequency,
                );
            }
            Command::SetFrequencyCorrection {
                result_sender,
                frequency_correction,
            } => {
                self.set_device_helper(
                    frequency_correction,
                    result_sender,
                    |state| &mut state.frequency_correction,
                    |_, _| {
                        tracing::warn!("todo: set frequency correction");
                        Ok(())
                    },
                    Event::FrequencyCorrection,
                );
            }
            Command::SetSampleRate {
                result_sender,
                sample_rate,
            } => {
                self.set_device_helper(
                    sample_rate,
                    result_sender,
                    |state| &mut state.sample_rate,
                    |_, _| {
                        tracing::warn!("todo: set sample rate");
                        Ok(())
                    },
                    Event::SampleRate,
                );
            }
            Command::SetTunerGain {
                result_sender,
                tuner_gain,
            } => {
                self.set_device_helper(
                    tuner_gain,
                    result_sender,
                    |state| &mut state.tuner_gain,
                    |_, _| {
                        tracing::warn!("todo: set tuner gain");
                        Ok(())
                    },
                    Event::TunerGain,
                );
            }
            Command::SetBiasTee {
                result_sender,
                bias_tee,
            } => {
                self.set_device_helper(
                    bias_tee,
                    result_sender,
                    |state| &mut state.bias_tee,
                    |_, _| {
                        tracing::warn!("todo: set bias tee");
                        Ok(())
                    },
                    Event::BiasTee,
                );
            }
        }
    }

    fn set_device_helper<T>(
        &mut self,
        new_value: T,
        result_sender: oneshot::Sender<Result<(), Error>>,
        state_mut: impl FnOnce(&mut State) -> &mut T,
        device_set: impl FnOnce(&mut mrrp_rtl_sdr::Device, T) -> Result<(), mrrp_rtl_sdr::Error>,
        event: impl FnOnce(T) -> Event,
    ) where
        T: PartialEq + Clone,
    {
        self.state_sender.send_if_modified(|state| {
            let current_value = state_mut(state);

            if *current_value == new_value {
                false
            }
            else if let Err(error) = device_set(&mut self.device, new_value.clone()) {
                let _ = result_sender.send(Err(error.into()));
                false
            }
            else {
                *current_value = new_value.clone();
                self.event_sender.send(event(new_value));
                let _ = result_sender.send(Ok(()));
                true
            }
        });
    }

    fn handle_data(&mut self) {
        todo!();
    }
}

#[derive(Debug)]
enum Command {
    Events {
        result_sender: oneshot::Sender<(State, broadcast::Receiver<Event>)>,
    },
    State {
        result_sender: oneshot::Sender<watch::Receiver<State>>,
    },
    SetCenterFrequency {
        result_sender: oneshot::Sender<Result<(), Error>>,
        center_frequency: u32,
    },
    SetFrequencyCorrection {
        result_sender: oneshot::Sender<Result<(), Error>>,
        frequency_correction: i32,
    },
    SetSampleRate {
        result_sender: oneshot::Sender<Result<(), Error>>,
        sample_rate: u32,
    },
    SetTunerGain {
        result_sender: oneshot::Sender<Result<(), Error>>,
        tuner_gain: TunerGain<i32>,
    },
    SetBiasTee {
        result_sender: oneshot::Sender<Result<(), Error>>,
        bias_tee: bool,
    },
}

#[derive(Clone, Debug)]
pub enum Event {
    CenterFrequency(u32),
    FrequencyCorrection(i32),
    SampleRate(u32),
    TunerGain(TunerGain<i32>),
    BiasTee(bool),
}

// todo: can't get DeviceDescriptor for this from RtlSdr. we would need to
// enumerate all device
#[derive(Clone, Debug)]
pub struct Info {
    //tuner_gains: Vec<i32>,
    //tuner_id: String,
}

#[derive(Clone, Debug)]
pub struct State {
    pub center_frequency: u32,
    pub frequency_correction: i32,
    pub sample_rate: u32,
    pub tuner_gain: TunerGain<i32>,
    pub bias_tee: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TunerGain<G> {
    Auto,
    Manual(G),
}

/*
impl From<TunerGain<i32>> for rtl_sdr_rs::TunerGain {
    fn from(value: TunerGain<i32>) -> Self {
        match value {
            TunerGain::Auto => rtl_sdr_rs::TunerGain::Auto,
            TunerGain::Manual(value) => rtl_sdr_rs::TunerGain::Manual(value),
        }
    }
}
     */
