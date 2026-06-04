use anyhow::Error;
use const_format::concatcp;
use egui::{
    FontFamily,
    Frame,
    Margin,
};
use futures_util::FutureExt;
use mrrp_widgets::frequency_dial::{
    FrequencyDial,
    FrequencyDialStyle,
};
use serde::{
    Deserialize,
    Serialize,
};
use tokio::task::JoinHandle;

use crate::{
    sdr::{
        GetSdrHandle,
        SourceHandle,
    },
    ui::error_message::PushErrorExt,
};

#[derive(Debug)]
pub struct RadioDockView<'a> {
    state: &'a mut RadioDockState,
}

impl<'a> RadioDockView<'a> {
    pub fn new(state: &'a mut RadioDockState) -> Self {
        Self { state }
    }

    pub fn show(self, ui: &mut egui::Ui) {
        match &mut self.state.inner {
            StateInner::Uninitialized => {
                // newly created dock, or loaded from file. either way we need to spawn a task
                // to enumerate devices

                self.state.inner = StateInner::Enumerating {
                    task: BackgroundTask::spawn(async move {
                        tracing::debug!("enumerating devices");
                        Ok(mrrp_rtl_sdr::enumerate_devices().await?.collect())
                    }),
                };

                // while that's running, show a spinner
                show_spinner(ui);
            }
            StateInner::Enumerating { task } => {
                match task.ready() {
                    None => {}
                    Some(Err(error)) => {
                        // something went wrong. open an error dialog
                        ui.push_error(error);

                        // we don't want to keep spinning (and opening error dialogs), so we
                        // switch to the enumerated state with no devices available.
                        self.state.inner = StateInner::Enumerated {
                            devices: vec![],
                            selection: None,
                        };
                    }
                    Some(Ok(devices)) => {
                        // got a device list

                        if devices.is_empty() {
                            tracing::debug!("no devices found");
                        }

                        for (i, device) in devices.iter().enumerate() {
                            tracing::debug!(i, name = device.name(), "device found");
                        }

                        // try to find preferred device
                        let selection =
                            self.state
                                .preferred_device
                                .as_ref()
                                .and_then(|preferred_device| {
                                    devices.iter().enumerate().find_map(|(i, device)| {
                                        device
                                            .serial_number()
                                            .is_some_and(|serial| serial == preferred_device)
                                            .then_some(i)
                                    })
                                });

                        tracing::debug!(?selection, "found preferred device");

                        self.state.inner = StateInner::Enumerated { devices, selection }
                    }
                }

                // always show a spinner in this state.
                show_spinner(ui);
            }
            StateInner::Enumerated { devices, selection } => {
                // show device selection

                if devices.is_empty() {
                    ui.label("No devices found");
                }
                else {
                    let available_width = ui.available_width();

                    for (i, device) in devices.iter().enumerate() {
                        // maybe format a prettier label. this could then be cached in a struct
                        let name = device.product_string().unwrap_or_else(|| device.name());
                        let selected = *selection == Some(i);

                        let button = egui::Button::new(name)
                            .selected(selected)
                            .frame_when_inactive(true)
                            .frame(true)
                            .wrap_mode(egui::TextWrapMode::Wrap)
                            .min_size([available_width, 0.0].into());

                        if ui.add(button).clicked() {
                            *selection = Some(i);
                        }
                    }
                }

                // we can't move a `&mut` to `self.state.inner` and a `&_` to to devices into
                // the closure passed to `horizontal`, so we only move the relevant bools.
                let enable_connect = selection.is_some();
                let (connect_clicked, cancel_clicked) = ui
                    .with_layout(egui::Layout::right_to_left(egui::Align::BOTTOM), |ui| {
                        egui::Frame::NONE
                            .inner_margin(6)
                            .show(ui, |ui| {
                                let cancel_clicked = ui
                                    .button(concatcp!(
                                        egui_phosphor::regular::ARROWS_COUNTER_CLOCKWISE,
                                        " Refresh"
                                    ))
                                    .clicked();

                                let connect_clicked = ui
                                    .add_enabled(
                                        enable_connect,
                                        egui::Button::new(concatcp!(
                                            egui_phosphor::regular::PLUGS_CONNECTED,
                                            " Connect",
                                        )),
                                    )
                                    .clicked();

                                (connect_clicked, cancel_clicked)
                            })
                            .inner
                    })
                    .inner;

                if connect_clicked {
                    if let Some(selection) = *selection
                        && let Some(device) = devices.get(selection)
                    {
                        let device = device.clone();
                        self.state.inner = StateInner::Connecting {
                            task: BackgroundTask::spawn(async move {
                                tracing::debug!(name = device.name(), "opening device");

                                Ok(device.open(Default::default()).await?)
                            }),
                        };
                    }
                    else {
                        tracing::warn!(
                            ?selection,
                            ?devices,
                            "connect clicked with invalid selection"
                        );
                    }
                }
                else if cancel_clicked {
                    self.state.inner = StateInner::Uninitialized;
                }
            }
            StateInner::Connecting { task } => {
                // we're connecting to the device

                match task.ready() {
                    None => {}
                    Some(Err(error)) => {
                        // something went wrong. open an error dialog
                        ui.push_error(error);

                        // we don't want to keep spinning (and opening error dialogs), so we
                        // switch to the uninitialized state
                        self.state.inner = StateInner::Uninitialized;
                    }
                    Some(Ok(device)) => {
                        // connected
                        let handle = ui.expect_sdr_handle().add_source(device);
                        self.state.inner = StateInner::Connected { handle };
                    }
                }

                show_spinner(ui);
            }
            StateInner::Connected { handle } => {
                // active source

                ui.add(egui::Label::new(
                    egui::WidgetText::from(&**handle.name()).strong(),
                ));

                // test
                let id = egui::Id::new("test_frequency");
                let mut frequency = ui.data(|data| data.get_temp(id).unwrap_or(7250000));

                // todo: use theme
                let response = Frame::dark_canvas(ui.style())
                    .inner_margin(Margin::symmetric(8, 4))
                    .show(ui, |ui| {
                        ui.take_available_width();
                        ui.add(
                            FrequencyDial::new(&mut frequency)
                                .insignificant_digits(3)
                                .desired_width(ui.available_width())
                                .style({
                                    // todo: remove this. instead let the user just configure a font
                                    // file - maybe through a
                                    // theme config
                                    let font_family = FontFamily::Name("dseg".into());

                                    let mut style = FrequencyDialStyle::from_egui(ui.style());
                                    style.font_id.family = font_family.clone();
                                    style.small_font_id.family = font_family;
                                    style.digit_spacing = 4.0;
                                    style.italics = true;
                                    style
                                }),
                        )
                    })
                    .inner;

                if response.changed() {
                    tracing::debug!(?frequency, "frequency changed");
                    ui.data_mut(|data| data.insert_temp(id, frequency));
                }

                ui.label("TODO: Radio");
            }
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct RadioDockState {
    /// this is the device we would like to pre-select if we can find it.
    preferred_device: Option<String>,

    #[serde(skip, default)]
    inner: StateInner,
}

#[derive(Debug, Default)]
enum StateInner {
    #[default]
    Uninitialized,
    Enumerating {
        task: BackgroundTask<Result<Vec<mrrp_rtl_sdr::DeviceInfo>, Error>>,
    },
    Enumerated {
        devices: Vec<mrrp_rtl_sdr::DeviceInfo>,
        selection: Option<usize>,
    },
    Connecting {
        task: BackgroundTask<Result<mrrp_rtl_sdr::Device, Error>>,
    },
    Connected {
        handle: SourceHandle,
    },
}

fn show_spinner(ui: &mut egui::Ui) {
    ui.spinner();
}

#[derive(Debug)]
pub struct BackgroundTask<T> {
    join_handle: JoinHandle<T>,
}

impl<T> BackgroundTask<T> {
    pub fn spawn<F>(future: F) -> Self
    where
        F: Future<Output = T> + Send + 'static,
        F::Output: Send + 'static,
    {
        Self {
            join_handle: tokio::spawn(future),
        }
    }

    pub fn ready(&mut self) -> Option<T> {
        // `now_or_never` is a bit inprecise here. we call now_or_never on
        // `&mut Future`, so we can continue to check it later.

        (&mut self.join_handle)
            .now_or_never()
            .transpose()
            .expect("A background task panicked")
    }
}
