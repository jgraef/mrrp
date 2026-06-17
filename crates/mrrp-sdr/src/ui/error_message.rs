use std::sync::Arc;

use anyhow::Error;
use egui::WidgetText;
use parking_lot::Mutex;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ErrorMessageState {
    #[serde(skip, default)]
    messages: Vec<ErrorMessage>,
}

impl ErrorMessageState {
    pub fn fill_from_context(&mut self, ctx: &egui::Context) {
        let queue = ctx.data_mut(|data| {
            // why does get_temp_mut_or_default need the type to be Clone??? argh
            data.get_temp_mut_or_default::<ErrorQueue>(egui::Id::NULL)
                .clone()
        });
        let mut queue = queue.0.lock();

        self.messages.extend(queue.drain(..));
    }
}

#[derive(Debug)]
pub struct ErrorMessageWindow<'a> {
    state: &'a mut ErrorMessageState,
}

impl<'a> ErrorMessageWindow<'a> {
    pub fn show(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("error_message");

        let mut window_open = !self.state.messages.is_empty();

        egui::Window::new("About mrrp-sdr")
            .id(id.with("window"))
            .vscroll(true)
            .hscroll(false)
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
            .collapsible(true)
            .open(&mut window_open)
            .show(ctx, |ui| {
                for error_message in &self.state.messages {
                    ui.collapsing(WidgetText::from(&error_message.message).monospace(), |ui| {
                        for cause in &error_message.chain {
                            ui.horizontal(|ui| {
                                ui.label("• ");
                                ui.label(WidgetText::from(cause).monospace());
                            });
                        }
                    });
                }
            });

        if !window_open {
            self.state.messages.clear();
        }
    }
}

#[derive(Debug)]
pub struct ErrorMessage {
    #[allow(unused)]
    error: Error,
    message: String,
    chain: Vec<String>,
}

impl From<Error> for ErrorMessage {
    fn from(error: Error) -> Self {
        let message = error.to_string();
        let chain = error.chain().map(|cause| cause.to_string()).collect();

        Self {
            error,
            message,
            chain,
        }
    }
}

pub trait PushErrorExt: Sized {
    fn push_error_message(self, error: ErrorMessage);

    fn push_error(self, error: impl Into<Error>) {
        let error = error.into();
        tracing::error!(?error);
        self.push_error_message(error.into());
    }
}

impl PushErrorExt for &egui::Context {
    fn push_error_message(self, error: ErrorMessage) {
        let queue = self.data_mut(|data| {
            // why does get_temp_mut_or_default need the type to be Clone??? argh
            data.get_temp_mut_or_default::<ErrorQueue>(egui::Id::NULL)
                .clone()
        });
        let mut queue = queue.0.lock();
        queue.push(error);
    }
}

impl PushErrorExt for &egui::Ui {
    fn push_error_message(self, error: ErrorMessage) {
        self.ctx().push_error_message(error);
    }
}

#[derive(Clone, Debug, Default)]
struct ErrorQueue(Arc<Mutex<Vec<ErrorMessage>>>);
