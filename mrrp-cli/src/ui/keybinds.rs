use std::{
    collections::HashMap,
    path::Path,
};

use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
};
use serde::Deserialize;

use crate::Error;

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

#[derive(Clone, Debug)]
pub struct Keybinds {
    keybinds: HashMap<Keybind, Action>,
}

impl Keybinds {
    pub fn get(&self, event: KeyEvent) -> Option<Action> {
        self.keybinds
            .get(&Keybind {
                code: event.code,
                modifiers: event.modifiers,
            })
            .copied()
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        tracing::debug!(path = %path.as_ref().display(), "Loading keybinds from file");
        Ok(toml::from_slice(&std::fs::read(path)?)?)
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
                (Keybind::from(KeyCode::Left).with_modifiers(KeyModifiers::SHIFT), Action::MoveLeftBig),
                (KeyCode::Right.into(), Action::MoveRight),
                (Keybind::from(KeyCode::Right).with_modifiers(KeyModifiers::SHIFT), Action::MoveRightBig),
                ('c'.into(), Action::CenterView),
            ]
                .into_iter()
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize)]
struct Keybind {
    code: KeyCode,
    modifiers: KeyModifiers,
}

impl Keybind {
    pub fn with_modifiers(mut self, modifiers: KeyModifiers) -> Self {
        self.modifiers |= modifiers;
        self
    }
}

impl From<KeyCode> for Keybind {
    fn from(value: KeyCode) -> Self {
        Self {
            code: value,
            modifiers: KeyModifiers::empty(),
        }
    }
}

impl From<char> for Keybind {
    fn from(value: char) -> Self {
        Self::from(KeyCode::Char(value))
    }
}

mod deserialize {
    use std::{
        collections::HashMap,
        fmt,
    };

    use serde::{
        Deserialize,
        Deserializer,
        de::{
            MapAccess,
            Visitor,
        },
    };

    use crate::ui::keybinds::{
        Action,
        Keybind,
        Keybinds,
    };

    struct KeybindsVisitor;

    impl<'de> Visitor<'de> for KeybindsVisitor {
        type Value = Keybinds;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a map, mapping actions to list of actions")
        }

        fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let mut keybinds = Keybinds {
                keybinds: HashMap::with_capacity(access.size_hint().unwrap_or_default()),
            };

            while let Some((action, keybind)) = access.next_entry::<Action, Vec<Keybind>>()? {
                for keybind in keybind {
                    keybinds.keybinds.insert(keybind, action);
                }
            }

            Ok(keybinds)
        }
    }

    impl<'de> Deserialize<'de> for Keybinds {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_map(KeybindsVisitor)
        }
    }
}
