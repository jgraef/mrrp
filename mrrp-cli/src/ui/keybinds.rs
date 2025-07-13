use std::collections::HashMap;

use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    Quit,
    ZoomIn,
    ZoomOut,
    MoveLeft,
    MoveLeftBig,
    MoveRight,
    MoveRightBig,
    CenterView,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(transparent)]
pub struct Keybinds {
    keybinds: HashMap<KeyBind, Action>,
}

impl Keybinds {
    pub fn get(&self, event: KeyEvent) -> Option<Action> {
        self.keybinds
            .get(&KeyBind {
                code: event.code,
                modifiers: event.modifiers,
            })
            .copied()
    }
}

impl Default for Keybinds {
    fn default() -> Self {
        Self {
            #[rustfmt::skip]
            keybinds: [
                ('q'.into(), Action::Quit),
                ('+'.into(), Action::ZoomIn),
                ('-'.into(), Action::ZoomOut),
                (KeyCode::Left.into(), Action::MoveLeft),
                (KeyBind::from(KeyCode::Left).with_modifiers(KeyModifiers::SHIFT), Action::MoveLeftBig),
                (KeyCode::Right.into(), Action::MoveRight),
                (KeyBind::from(KeyCode::Right).with_modifiers(KeyModifiers::SHIFT), Action::MoveRightBig),
                ('c'.into(), Action::CenterView),
            ]
                .into_iter()
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
struct KeyBind {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl KeyBind {
    pub fn with_modifiers(mut self, modifiers: KeyModifiers) -> Self {
        self.modifiers |= modifiers;
        self
    }
}

impl From<KeyCode> for KeyBind {
    fn from(value: KeyCode) -> Self {
        Self {
            code: value,
            modifiers: KeyModifiers::empty(),
        }
    }
}

impl From<char> for KeyBind {
    fn from(value: char) -> Self {
        Self::from(KeyCode::Char(value))
    }
}
