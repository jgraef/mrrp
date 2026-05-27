pub mod sink;
pub mod source;

use tokio::sync::mpsc;

use crate::sdr::{
    sink::SpectrumSink,
    source::AsyncReadSamples,
};

#[derive(derive_more::Debug)]
pub struct SdrRuntime {
    command_receiver: mpsc::UnboundedReceiver<Command>,

    #[debug(skip)]
    source: Option<Box<dyn AsyncReadSamples + Send>>,

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

        SdrHandle { command_sender }
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
            Command::Link { sink } => {
                self.spectrum_sinks.push(sink);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct SdrHandle {
    command_sender: mpsc::UnboundedSender<Command>,
}

impl SdrHandle {
    pub fn link<S>(&self, sink: S) -> SdrLinkHandle
    where
        S: SpectrumSink + Send,
    {
        // todo: for now we only accept `SpectrumSink`s. Do we want one generic
        // method? Or separate for sinks/sources/etc.?

        //todo!();
        SdrLinkHandle {}
    }
}

#[derive(derive_more::Debug)]
enum Command {
    Link {
        #[debug(skip)]
        sink: Box<dyn SpectrumSink + Send>,
    },
}

#[derive(Clone, Debug)]
pub struct SdrLinkHandle {
    // todo: do we even need this handle? for now the option it'll be put in is basically a bool
    // telling the dock if it's linked. but we might want this later.
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
