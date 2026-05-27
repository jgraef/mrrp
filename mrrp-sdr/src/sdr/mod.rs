pub mod sink;

use std::sync::Arc;

use mrrp::io::AsyncReadSamples;
use num_complex::Complex;
use tokio::sync::mpsc;

use crate::{
    sdr::sink::SpectrumSink,
    util::AtomicIds,
};

pub type Iq = Complex<f32>;

#[derive(derive_more::Debug)]
pub struct SdrRuntime {
    command_receiver: mpsc::UnboundedReceiver<Command>,

    #[debug(skip)]
    source: Option<Box<dyn AsyncReadSamples<Iq, Error = Box<dyn std::error::Error>> + Send>>,

    #[debug(skip)]
    spectrum_sinks: Vec<Box<dyn SpectrumSink + Send>>,
}

impl SdrRuntime {
    pub fn spawn() -> SdrHandle {
        let (command_sender, command_receiver) = mpsc::unbounded_channel();

        let this = Self {
            command_receiver,
            source: None,
            spectrum_sinks: vec![],
        };

        let _join_handle = tokio::spawn(this.run());

        SdrHandle {
            command_sender,
            handle_ids: Default::default(),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                biased;
                command = self.command_receiver.recv() => {
                    let Some(command) = command else { break; };
                    self.handle_command(command).await;
                }
            }
        }
    }

    async fn handle_command(&mut self, command: Command) {
        match command {
            Command::AddSpectrumSink {
                id,
                spectrum_sink: sink,
            } => {
                self.spectrum_sinks.push(sink);
            }
            Command::AddSource { id, source } => {
                self.source = Some(source);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct SdrHandle {
    command_sender: mpsc::UnboundedSender<Command>,
    handle_ids: Arc<AtomicIds>,
}

impl SdrHandle {
    fn send_command(&self, command: Command) {
        self.command_sender
            .send(command)
            .expect("SDR runtime command channel closed");
    }

    pub fn add_spectrum_sink<S>(&self, sink: S) -> SpectrumSinkHandle
    where
        S: SpectrumSink + Send + 'static,
    {
        let id = self.handle_ids.next();

        self.send_command(Command::AddSpectrumSink {
            id,
            spectrum_sink: Box::new(sink),
        });

        SpectrumSinkHandle {
            command_sender: self.command_sender.clone(),
            id,
        }
    }

    pub fn add_source<S>(&self, source: S) -> SourceHandle
    where
        S: AsyncReadSamples<Iq> + Sized + Send + 'static,
        S::Error: std::error::Error + Sized + Send + Sync + 'static,
    {
        let id = self.handle_ids.next();

        /*self.send_command(Command::AddSource {
            id,
            //source: Box::new(source.map_err(|error| Box::new(error) as Box<dyn
            // std::error::Error>)),
            source: Box::new(source.map_err(test_box_error)),
        });*/

        SourceHandle {
            command_sender: self.command_sender.clone(),
            id,
        }
    }
}

#[derive(derive_more::Debug)]
enum Command {
    AddSpectrumSink {
        id: usize,
        #[debug(skip)]
        spectrum_sink: Box<dyn SpectrumSink + Send>,
    },
    AddSource {
        id: usize,
        #[debug(skip)]
        source: Box<dyn AsyncReadSamples<Iq, Error = Box<dyn std::error::Error>> + Send>,
    },
}

#[derive(Clone, Debug)]
pub struct SpectrumSinkHandle {
    command_sender: mpsc::UnboundedSender<Command>,
    id: usize,
}

#[derive(Clone, Debug)]
pub struct SourceHandle {
    command_sender: mpsc::UnboundedSender<Command>,
    id: usize,
}

pub trait GetSdrHandle {
    fn sdr_handle(&self) -> Option<SdrHandle>;

    fn expect_sdr_handle(&self) -> SdrHandle {
        self.sdr_handle()
            .expect("Could not retrieve handle to SDR runtime")
    }
}

impl GetSdrHandle for egui::Context {
    fn sdr_handle(&self) -> Option<SdrHandle> {
        self.data(|data| data.get_temp(egui::Id::NULL))
    }
}

pub fn initialize_sdr_runtime(ctx: &egui::Context) {
    let sdr_handle = SdrRuntime::spawn();
    ctx.data_mut(|data| data.insert_temp(egui::Id::NULL, sdr_handle));
}

fn test_box_error<E>(error: E) -> Box<dyn std::error::Error>
where
    E: std::error::Error + 'static,
{
    Box::new(error)
}
