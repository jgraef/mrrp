use egui_dock::{
    DockState,
    NodePath,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    cli::UiCommand,
    ui::dock::{
        Tab,
        default_dock_state,
    },
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppState {
    /// If this state should be persisted
    ///
    /// This value is not serialized, but determined by whether the
    /// deserialization worked.
    #[serde(skip, default)]
    pub persist: bool,

    /// Persist all app state - even settings that are usually reset on startup,
    /// e.g. which windows are open. This is useful for debugging.
    ///
    /// Note: Actually we always persist this whole struct, but when loading
    /// we'll reset certain states if this is false.
    #[serde(default = "crate::util::bool_true")]
    pub persist_everything: bool,

    #[serde(skip, default)]
    pub show_about_window: bool,

    #[serde(skip, default)]
    pub show_debug_window: bool,

    #[serde(default = "default_dock_state")]
    pub dock_state: DockState<Tab>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            persist: true,
            persist_everything: true,
            show_about_window: false,
            show_debug_window: false,
            dock_state: default_dock_state(),
        }
    }
}

impl AppState {
    const KEY: &str = "app_state";

    pub fn load(storage: &dyn eframe::Storage, command: &UiCommand) -> Self {
        tracing::debug!("loading app state");

        let mut state = if command.reset_app_state {
            AppState::default()
        }
        else {
            let mut state = if let Some(value) = storage.get_string(Self::KEY) {
                match serde_json::from_str::<Self>(&value) {
                    Ok(mut state) => {
                        state.persist = true;
                        state
                    }
                    Err(error) => {
                        tracing::error!(%error, "Failed to load app state. Using default state, but will not persist it. Use --reset-state to reset.");
                        let mut state = Self::default();
                        state.persist = false;
                        state
                    }
                }
            }
            else {
                tracing::debug!("No app state present. Using default");
                let mut state = Self::default();
                state.persist = true;
                state
            };

            if !state.persist_everything {
                state.reset_on_app_start();
            }

            state
        };

        if command.dont_save_app_state {
            state.persist = false;
        }

        state
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        if self.persist {
            tracing::debug!("saving app state");
            let value = serde_json::to_string(self).expect("main app state serialization");
            storage.set_string(Self::KEY, value);
        }
    }

    fn reset_on_app_start(&mut self) {
        self.show_about_window = false;
        self.show_debug_window = false;
    }
}

#[derive(derive_more::Debug, Default)]

pub struct CommandBuffer {
    #[debug(skip)]
    commands: Vec<Box<dyn FnOnce(&mut AppState)>>,
}

impl CommandBuffer {
    pub fn push(&mut self, command: impl FnOnce(&mut AppState) + 'static) {
        self.commands.push(Box::new(command));
    }

    pub fn apply(&mut self, app_state: &mut AppState) {
        for command in self.commands.drain(..) {
            command(app_state);
        }
    }

    pub fn add_dock(&mut self, path: Option<NodePath>, tab: Tab) {
        self.push(move |state| {
            if let Some(path) = path {
                if let Ok(leaf) = state.dock_state.leaf_mut(path) {
                    leaf.append_tab(tab);
                }
            }
            else {
                state.dock_state.push_to_focused_leaf(tab);
            }
        })
    }
}
