use std::collections::HashSet;

use egui::{
    Checkbox,
    Grid,
};

use crate::{
    cli::UiCommand,
    config::{
        Config,
        RadioConfig,
    },
    hal::{
        self,
        radio::RadioDescriptor,
    },
};

#[derive(Debug)]
pub struct RadioUiState {
    radios: Vec<Radio>,
    selected: Option<usize>,
    connected: Option<Connected>,

    config_window: Option<RadioConfigWindowState>,
}

#[derive(Clone, Debug)]
struct Radio {
    name: String,
    config: Option<RadioConfig>,
    descriptor: Option<RadioDescriptor>,
    configured: bool,
    detected: bool,
}

#[derive(Clone, Copy, Debug)]
struct Connected {
    in_progress: bool,
    radio: usize,
}

impl RadioUiState {
    pub fn new(config: &Config, command: &UiCommand) -> Self {
        let mut radios = vec![];
        let mut selected = None;

        let mut matched_radios = HashSet::new();

        match hal::radio::list_devices() {
            Ok(descriptors) => {
                for descriptor in descriptors {
                    tracing::debug!(?descriptor, "found radio");

                    let matched =
                        config
                            .radios
                            .iter()
                            .enumerate()
                            .find_map(|(index, (name, config))| {
                                descriptor
                                    .matches(config)
                                    .then(|| (index, name.clone(), config.clone()))
                            });

                    if let Some((index, name, config)) = &matched {
                        tracing::debug!(%name, ?config, "matched radio to config");

                        matched_radios.insert(*index);

                        radios.push(Radio {
                            name: name.clone(),
                            config: Some(config.clone()),
                            descriptor: Some(descriptor),
                            configured: true,
                            detected: true,
                        });
                    }
                    else {
                        let name = descriptor.name();
                        tracing::debug!(%name, "didn't find a matching configuration for radio");

                        radios.push(Radio {
                            name,
                            config: None,
                            descriptor: Some(descriptor),
                            configured: false,
                            detected: true,
                        });
                    }
                }
            }
            Err(error) => {
                tracing::error!(%error, "Failed to enumerate radios");
            }
        }

        for (index, (name, config)) in config.radios.iter().enumerate() {
            if !matched_radios.contains(&index) {
                tracing::debug!(%name, ?config, "radio configured, but not detected");

                radios.push(Radio {
                    name: name.clone(),
                    config: Some(config.clone()),
                    descriptor: None,
                    configured: true,
                    detected: false,
                })
            }
        }

        // sort radios
        radios.sort_by(|left, right| {
            // (!radio.configured, !radio.detected, &radio.name)
            left.configured
                .cmp(&right.configured)
                .reverse()
                .then(left.detected.cmp(&right.detected).reverse())
                .then(left.name.cmp(&right.name))
        });

        // apply selection from command line
        if let Some(name) = &command.radio {
            if let Some((index, radio)) = radios
                .iter()
                .enumerate()
                .find(|(_index, radio)| &radio.name == name)
            {
                tracing::debug!(?radio, "radio selected by command line");
                selected = Some(index);
            }
            else {
                tracing::warn!(name = %name, "radio selected by command line not found");
            }
        }

        // select first, if nothing is selected already, and there is at least one radio
        if selected.is_none() && !radios.is_empty() {
            selected = Some(0);
        }

        Self {
            radios,
            selected,
            connected: None,
            config_window: None,
        }
    }
}

pub struct RadioUi<'a> {
    state: &'a mut RadioUiState,
}

impl<'a> RadioUi<'a> {
    pub fn new(state: &'a mut RadioUiState) -> Self {
        Self { state }
    }
}

impl<'a> egui::Widget for RadioUi<'a> {
    fn ui(self, ui: &mut egui::Ui) -> egui::Response {
        let mut response = ui
            .add_enabled_ui(self.state.connected.is_none(), |ui| {
                egui::ComboBox::new("radio_dropdown", "")
                    .wrap_mode(egui::TextWrapMode::Truncate)
                    .width(ui.available_width())
                    .selected_text(
                        self.state
                            .selected
                            .map_or("", |index| &self.state.radios[index].name),
                    )
                    .show_ui(ui, |ui| {
                        for (index, radio) in self.state.radios.iter().enumerate() {
                            let configured_atom = if radio.configured {
                                egui_phosphor::regular::STAR
                            }
                            else {
                                ""
                            };

                            let detected_atom = if !radio.detected {
                                egui_phosphor::regular::EXCLAMATION_MARK
                            }
                            else {
                                ""
                            };

                            let button = egui::Button::selectable(
                                self.state.selected == Some(index),
                                (&radio.name, configured_atom, detected_atom),
                            );

                            // todo: some configurations can't be detected, so they need to be
                            // always enabled (e.g. RTL-TCP)
                            let enabled = radio.detected;

                            let response = ui
                                .add_enabled(enabled, button)
                                .on_hover_text(format!("{radio:#?}"));

                            if enabled && response.clicked() && Some(index) != self.state.selected {
                                self.state.selected = Some(index);
                            }
                        }
                    });
            })
            .response;

        response |= ui
            .horizontal(|ui| {
                if let Some(connected) = self.state.connected {
                    if connected.in_progress {
                        ui.spinner();
                    }

                    let disconnect_button =
                        egui::Button::new((egui_phosphor::regular::STOP, "Disconnect")).small();

                    let response = ui.add(disconnect_button).on_hover_ui(|ui| {
                        ui.label(format!(
                            "Disconnect from radio \"{}\"",
                            self.state.radios[connected.radio].name
                        ));
                    });

                    if response.clicked() {
                        self.state.connected = None;
                        // todo
                    };
                }
                else {
                    let connect_button =
                        egui::Button::new((egui_phosphor::regular::PLAY, "Connect")).small();

                    let response = ui
                        .add_enabled(self.state.selected.is_some(), connect_button)
                        .on_hover_ui(|ui| {
                            if let Some(index) = self.state.selected {
                                ui.label(format!(
                                    "Connect to radio \"{}\"",
                                    self.state.radios[index].name
                                ));
                            }
                            else {
                                ui.label(format!("No radio selected"));
                            }
                        });

                    if response.clicked() {

                        // todo
                    };
                }

                let configure_button =
                    egui::Button::new((egui_phosphor::regular::WRENCH, "Configure")).small();

                let response = ui
                    .add_enabled(self.state.selected.is_some(), configure_button)
                    .on_hover_ui(|ui| {
                        if let Some(index) = self.state.selected {
                            ui.label(format!(
                                "Configure radio \"{}\"",
                                self.state.radios[index].name
                            ));
                        }
                        else {
                            ui.label(format!("No radio selected"));
                        }
                    });

                if response.clicked()
                    && self.state.config_window.is_none()
                    && let Some(radio) = self.state.selected
                {
                    let mut config_window_state = RadioConfigWindowState {
                        radio,
                        name: self.state.radios[radio].name.clone(),
                        ty: Default::default(),
                        rtl_sdr: Default::default(),
                    };

                    if let Some(config) = &self.state.radios[radio].config {
                        config_window_state.fill_from_config(config);
                    }
                    else {
                        config_window_state.name = self.state.radios[radio].name.clone();

                        // todo: this depends on the device.
                        config_window_state.rtl_sdr.sample_rate = "2400000".to_owned();
                    }

                    self.state.config_window = Some(config_window_state);
                };
            })
            .response;

        response
    }
}

#[derive(Debug)]
pub struct RadioConfigWindowState {
    radio: usize,

    name: String,
    ty: RadioConfigWindowType,

    rtl_sdr: RadioConfigWindowStateRtlSdr,
}

impl RadioConfigWindowState {
    fn fill_from_config(&mut self, config: &RadioConfig) {
        match config {
            RadioConfig::RtlSdr {
                filter,
                sample_rate,
                bias_tee,
            } => {
                self.rtl_sdr.index = filter
                    .index
                    .map(|index| index.to_string())
                    .unwrap_or_default();
                self.rtl_sdr.vendor_id = filter
                    .vendor_id
                    .map(|vendor_id| format!("0x{vendor_id:04x}"))
                    .unwrap_or_default();
                self.rtl_sdr.product_id = filter
                    .product_id
                    .map(|product_id| format!("0x{product_id:04x}"))
                    .unwrap_or_default();
                self.rtl_sdr.manufacturer = filter.manufacturer.clone().unwrap_or_default();
                self.rtl_sdr.product = filter.product.clone().unwrap_or_default();
                self.rtl_sdr.serial = filter.serial.clone().unwrap_or_default();

                self.rtl_sdr.sample_rate = sample_rate
                    .map(|sample_rate| sample_rate.to_string())
                    .unwrap_or_default();

                self.rtl_sdr.bias_tee = *bias_tee;
            }
            _ => {
                // todo
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum RadioConfigWindowType {
    #[default]
    RtlSdr,
    RtlTcp,
    SoundCard,
    RawNetwork,
}

impl RadioConfigWindowType {
    const ALL: &[Self] = &[
        Self::RtlSdr,
        Self::RtlTcp,
        Self::SoundCard,
        Self::RawNetwork,
    ];

    fn display_name(&self) -> &'static str {
        match self {
            RadioConfigWindowType::RtlSdr => "RTL-SDR",
            RadioConfigWindowType::RtlTcp => "RTL-TCP",
            RadioConfigWindowType::SoundCard => "Sound Card",
            RadioConfigWindowType::RawNetwork => "Network (raw IQ)",
        }
    }
}

#[derive(Debug, Default)]
struct RadioConfigWindowStateRtlSdr {
    // filter
    index: String,
    vendor_id: String,
    product_id: String,
    manufacturer: String,
    product: String,
    serial: String,

    // options
    sample_rate: String,
    bias_tee: bool,
}

#[derive(Debug)]
pub struct RadioConfigWindow<'a> {
    state: &'a mut RadioUiState,
}

impl<'a> RadioConfigWindow<'a> {
    pub fn new(state: &'a mut RadioUiState) -> Self {
        Self { state }
    }

    pub fn show(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("radio_config");

        if let Some(config_window) = &mut self.state.config_window {
            let radio = &mut self.state.radios[config_window.radio];
            let mut is_open = true;
            let mut cancelled = false;

            egui::Window::new(&radio.name)
                .id(id.with("window"))
                .open(&mut is_open)
                .vscroll(true)
                .collapsible(false)
                .show(ctx, |ui| {
                    let mut response = Grid::new(id.with("grid_header"))
                        .show(ui, |ui| {
                            ui.label("Name");
                            ui.add(
                                egui::TextEdit::singleline(&mut config_window.name)
                                    .desired_width(f32::INFINITY),
                            );
                            ui.end_row();

                            ui.label("Type");
                            egui::ComboBox::from_id_salt(id.with("type_dropdown"))
                                .selected_text(config_window.ty.display_name())
                                .show_ui(ui, |ui| {
                                    for &ty in RadioConfigWindowType::ALL {
                                        ui.selectable_value(
                                            &mut config_window.ty,
                                            ty,
                                            ty.display_name(),
                                        );
                                    }
                                });
                            ui.end_row();
                        })
                        .response;

                    ui.separator();

                    match config_window.ty {
                        RadioConfigWindowType::RtlSdr => {
                            ui.collapsing("Device Identification", |ui| {
                                response |= Grid::new(id.with("grid_id"))
                                    .show(ui, |ui| {
                                        ui.label("Device Index");
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut config_window.rtl_sdr.index,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                        ui.end_row();

                                        ui.label("Vendor ID");
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut config_window.rtl_sdr.vendor_id,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                        ui.end_row();

                                        ui.label("Product ID");
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut config_window.rtl_sdr.product_id,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                        ui.end_row();

                                        ui.label("Manufacturer Name");
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut config_window.rtl_sdr.manufacturer,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                        ui.end_row();

                                        ui.label("Product Name");
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut config_window.rtl_sdr.product,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                        ui.end_row();

                                        ui.label("Serial Number");
                                        ui.add(
                                            egui::TextEdit::singleline(
                                                &mut config_window.rtl_sdr.serial,
                                            )
                                            .desired_width(f32::INFINITY),
                                        );
                                        ui.end_row();
                                    })
                                    .response;
                            });

                            response |= Grid::new(id.with("grid_options"))
                                .show(ui, |ui| {
                                    ui.label("Sample Rate");
                                    ui.add(
                                        egui::TextEdit::singleline(
                                            &mut config_window.rtl_sdr.sample_rate,
                                        )
                                        .desired_width(f32::INFINITY),
                                    );
                                    ui.end_row();

                                    ui.label("Bias Tee");
                                    ui.add(Checkbox::without_text(
                                        &mut config_window.rtl_sdr.bias_tee,
                                    ));
                                    ui.end_row();
                                })
                                .response;
                        }
                        RadioConfigWindowType::RtlTcp => {
                            // todo
                        }
                        RadioConfigWindowType::SoundCard => {
                            // todo
                        }
                        RadioConfigWindowType::RawNetwork => {
                            // todo
                        }
                    };

                    response |= ui
                        .with_layout(egui::Layout::right_to_left(egui::Align::BOTTOM), |ui| {
                            if ui.button("Save").clicked() {
                                // todo
                            }

                            if ui.button("Cancel").clicked() {
                                cancelled = true;
                            }
                        })
                        .response;

                    response
                });

            if !is_open || cancelled {
                self.state.config_window = None;
            }
        }
    }
}
