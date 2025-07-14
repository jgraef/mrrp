use std::{
    fmt::Debug,
    time::Duration,
};

use chrono::{
    DateTime,
    Local,
};
use color_eyre::eyre::{
    Error,
    bail,
};
use crossterm::execute;
use futures_util::TryStreamExt;
use ratatui::DefaultTerminal;
use rtlsdr_async::Backend;
use serde::{
    Deserialize,
    Serialize,
};
use tokio::{
    sync::mpsc,
    time::Interval,
};

use crate::{
    args::MainArgs,
    fft::Fft,
    files::AppFiles,
    reader::SampleReader,
    ui::{
        Ui,
        UiEvent,
        UiState,
        UiWidget,
        bandplan::Bandplan,
        keybinds::Keybinds,
    },
    util::FrequencyBand,
};

const DEFAULT_CENTER_FREQUENCY: u32 = 7_000_000;
const DEFAULT_SAMPLE_RATE: u32 = 2_400_000;

#[derive(Debug, Serialize, Deserialize)]
pub struct AppState {
    ui_state: UiState,
    sampled_frequency_band: FrequencyBand,
}

#[derive(Debug)]
pub struct App<B> {
    state: AppState,
    files: AppFiles,
    app_events: mpsc::UnboundedReceiver<AppEvent>,
    proxy: AppProxy,
    scroll_interval: Interval,
    rtl_sdr: B,
    sample_reader: SampleReader,
    fft: Fft,
    terminal: DefaultTerminal,
    terminal_events: crossterm::event::EventStream,
    ui: Ui,
    exit_requested: bool,
    redraw_interval: Interval,
}

impl<B> App<B>
where
    B: Backend + Send + Clone + 'static,
    <B as Backend>::Error: std::error::Error + Send + Sync + 'static,
{
    pub async fn new(args: MainArgs, app_files: AppFiles, rtl_sdr: B) -> Result<Self, Error> {
        if args.fft_size == 0 {
            bail!("FFT size must be greater than 0");
        }
        if args.fft_size & 1 == 1 {
            bail!("FFT size must be a multiple of 2")
        }
        if args.fft_overlap >= args.fft_size {
            bail!("FFT overlap must be less than FFT size");
        }

        // todo: load config here

        let keybinds = if let Some(path) = &args.keybinds {
            Keybinds::from_path(path)?
        }
        else {
            app_files.keybinds()?
        };

        let bandplan = if let Some(path) = &args.bandplan {
            Bandplan::from_path(path)?
        }
        else {
            app_files.bandplan()?
        };

        let mut state = (!args.reset)
            .then(|| {
                app_files
                    .load_app_state()
                    .inspect_err(|error| {
                        tracing::warn!(?error, "Failed to load previous app state")
                    })
                    .ok()
                    .map(|snapshot| {
                        // currently we only care about the actual state, but we intend to store
                        // more metadata in the snapshot, like timestamp, version number, etc.
                        snapshot.app_state
                    })
            })
            .flatten()
            .unwrap_or_else(|| {
                let sampled_frequency_band = FrequencyBand::from_center_and_bandwidth(
                    args.frequency.unwrap_or(DEFAULT_CENTER_FREQUENCY),
                    args.sample_rate.unwrap_or(DEFAULT_SAMPLE_RATE),
                );
                AppState {
                    ui_state: UiState::new(sampled_frequency_band),
                    sampled_frequency_band,
                }
            });

        if let Some(center_frequency) = args.frequency {
            if state.sampled_frequency_band.center() != center_frequency {
                state.sampled_frequency_band = FrequencyBand::from_center_and_bandwidth(
                    center_frequency,
                    state.sampled_frequency_band.bandwidth(),
                )
            }
        }
        if let Some(sample_rate) = args.sample_rate {
            if sample_rate & 1 == 1 {
                // todo: we currently can't calculate the start and end frequency of the signal
                // correctly in this case.
                bail!("Sample rate must be divisble by 2");
            }

            if state.sampled_frequency_band.bandwidth() != sample_rate {
                state.sampled_frequency_band = FrequencyBand::from_center_and_bandwidth(
                    state.sampled_frequency_band.center(),
                    sample_rate,
                );
            }
        }

        rtl_sdr
            .set_center_frequency(state.sampled_frequency_band.center())
            .await?;
        rtl_sdr
            .set_sample_rate(state.sampled_frequency_band.bandwidth())
            .await?;
        rtl_sdr.set_tuner_gain(args.gain.into()).await?;

        let sample_reader =
            SampleReader::new(rtl_sdr.samples().await?, args.fft_size, args.fft_overlap);

        let terminal = ratatui::init();
        crossterm::execute!(std::io::stdout(), crossterm::event::EnableMouseCapture)?;

        let terminal_events = crossterm::event::EventStream::new();

        let (event_sender, event_receiver) = mpsc::unbounded_channel();

        let ui = Ui::new(state.sampled_frequency_band, keybinds, bandplan);

        Ok(Self {
            state,
            files: app_files,
            app_events: event_receiver,
            proxy: AppProxy { event_sender },
            scroll_interval: tokio::time::interval(Duration::from_millis(args.scroll_interval)),
            rtl_sdr,
            sample_reader,
            fft: Fft::new(args.fft_size, args.fft_window),
            terminal,
            terminal_events,
            ui,
            exit_requested: false,
            redraw_interval: tokio::time::interval(Duration::from_millis(args.redraw_interval)),
        })
    }

    pub async fn run(&mut self) -> Result<(), Error> {
        self.exit_requested = false;

        while !self.exit_requested {
            tokio::select! {
                option = self.app_events.recv() => {
                    let Some(event) = option
                    else {
                        break;
                    };

                    self.handle_event(event)?;
                }
                result = self.terminal_events.try_next() => {
                    let Some(event) = result?
                    else {
                        break;
                    };
                    self.ui.handle_event(UiEvent::Terminal(event), &mut self.proxy, &mut self.state.ui_state);
                }
                _ = self.redraw_interval.tick() => {
                    self.terminal.draw(|frame| frame.render_widget(UiWidget { ui: &mut self.ui, state: &mut self.state.ui_state}, frame.area()))?;
                }
                _ = self.scroll_interval.tick() => {
                    self.ui.handle_event(UiEvent::ScrollWaterfall, &mut self.proxy, &mut self.state.ui_state);
                }
                result = self.sample_reader.read() => {
                    let Some(samples) = result?
                    else {
                        tracing::warn!("sample stream stopped");
                        break;
                    };

                    let spectrum = self.fft.forward(samples);
                    self.ui.handle_event(UiEvent::Spectrum { spectrum, frequency_band: self.state.sampled_frequency_band }, &mut self.proxy, &mut self.state.ui_state);
                }
            }
        }

        Ok(())
    }

    pub fn persist(self) -> Result<(), Error> {
        self.files.save_app_state(AppSnapshot {
            app_state: &self.state,
            timestamp: Local::now(),
        })?;
        Ok(())
    }

    fn handle_event(&mut self, event: AppEvent) -> Result<(), Error> {
        match event {
            AppEvent::Error { error } => return Err(error),
            AppEvent::RequestExit => {
                self.exit_requested = true;
            }
            AppEvent::SetScrollInterval { interval } => {
                self.scroll_interval = tokio::time::interval(interval);
            }
            AppEvent::SetCenterFrequency { frequency } => {
                let rtl_sdr = self.rtl_sdr.clone();
                let event_sender = self.proxy.event_sender.clone();

                let sampled_frequency_band = FrequencyBand::from_center_and_bandwidth(
                    frequency,
                    self.state.sampled_frequency_band.bandwidth(),
                );

                tokio::spawn(async move {
                    if let Err(error) = rtl_sdr.set_center_frequency(frequency).await {
                        let _ = event_sender.send(AppEvent::Error {
                            error: error.into(),
                        });
                    }
                    else {
                        let _ = event_sender.send(AppEvent::SampledFrequencyBandChanged {
                            sampled_frequency_band,
                        });
                    }
                });
            }
            AppEvent::SampledFrequencyBandChanged {
                sampled_frequency_band,
            } => {
                self.state.sampled_frequency_band = sampled_frequency_band;
            }
        }

        Ok(())
    }
}

impl<B> Drop for App<B> {
    fn drop(&mut self) {
        let _ = execute!(std::io::stdout(), crossterm::event::DisableMouseCapture);
        ratatui::restore();
    }
}

#[derive(Clone, Debug)]
pub struct AppProxy {
    event_sender: mpsc::UnboundedSender<AppEvent>,
}

impl AppProxy {
    pub fn request_exit(&self) {
        let _ = self.event_sender.send(AppEvent::RequestExit);
    }

    pub fn set_scroll_interval(&mut self, interval: Duration) {
        let _ = self
            .event_sender
            .send(AppEvent::SetScrollInterval { interval });
    }

    pub fn set_center_frequency(&mut self, frequency: u32) {
        let _ = self
            .event_sender
            .send(AppEvent::SetCenterFrequency { frequency });
    }
}

#[derive(Debug)]
enum AppEvent {
    Error {
        error: Error,
    },
    RequestExit,
    SetScrollInterval {
        interval: Duration,
    },
    SetCenterFrequency {
        frequency: u32,
    },
    SampledFrequencyBandChanged {
        sampled_frequency_band: FrequencyBand,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AppSnapshot<A> {
    pub app_state: A,
    pub timestamp: DateTime<Local>,
}
