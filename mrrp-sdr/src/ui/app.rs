use eframe::Storage;
use tracing::span::EnteredSpan;

use crate::{
    cli::UiCommand,
    sdr::{
        initialize_sdr_runtime,
        source::{
            LoopedFileSource,
            MockSource,
            SourceInfo,
        },
    },
    ui::{
        about_window::AboutWindow,
        debug_window::DebugWindow,
        dock::DockPanel,
        menu::MainMenuPanel,
        state::{
            AppState,
            CommandBuffer,
        },
    },
};

#[derive(Debug)]
pub struct App {
    /// app state. this will be serialized and stored. some of it may be reset
    /// when the app is loaded.
    app_state: AppState,

    /// buffer for deferred commands that can mutate the app state
    command_buffer: CommandBuffer,

    #[allow(unused)]
    span: EnteredSpan,
}

impl App {
    pub fn new(command: UiCommand, ctx: &egui::Context, storage: &dyn Storage) -> Self {
        // start SDR runtime
        let sdr = initialize_sdr_runtime(ctx);

        // tracing span for whole app (not sdr runtime)
        let span = tracing::info_span!("app").entered();

        // configure style
        ctx.all_styles_mut(|style| {
            style.url_in_tooltip = true;
        });

        // create a nock source
        let center_frequency = command.center_frequency.unwrap_or(7000000.0);
        let sample_rate = command.center_frequency.unwrap_or(2400000.0);

        let source = if let Some(test_file) = &command.file {
            tracing::debug!(?center_frequency, "test: LoopedFileSource");
            sdr.add_source(
                LoopedFileSource::new(test_file)
                    .unwrap()
                    .with_center_frequency(center_frequency),
            )
        }
        else {
            tracing::debug!(?center_frequency, ?sample_rate, "test: noise");
            sdr.add_source(MockSource::new(SourceInfo {
                center_frequency,
                sample_rate,
            }))
        };
        source.leak();

        // load app state
        let app_state = AppState::load(storage, &command);

        Self {
            app_state,
            command_buffer: Default::default(),
            span,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // app menu
        ui.add(MainMenuPanel::new(
            &mut self.app_state,
            &mut self.command_buffer,
        ));

        // main panel with docks
        ui.add(DockPanel::new(
            &mut self.app_state,
            &mut self.command_buffer,
        ));

        // show windows
        //RadioConfigWindow::new(&mut self.radio_state).show(ui.ctx());
        AboutWindow::new(&mut self.app_state).show(ui.ctx());
        DebugWindow::new(&mut self.app_state).show(ui.ctx());

        // apply deferred commands
        self.command_buffer.apply(&mut self.app_state);
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app_state.save(storage);
    }
}
