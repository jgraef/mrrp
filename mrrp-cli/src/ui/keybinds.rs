use std::{
    collections::HashMap,
    fs::File,
    io::{
        BufReader,
        BufWriter,
    },
    path::Path,
};

use crossterm::event::{
    KeyCode,
    KeyEvent,
    KeyModifiers,
};
use serde::{
    Deserialize,
    Serialize,
};

use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    TuneToView,
    Test,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Keybinds {
    #[serde(with = "serde_keybinds")]
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
        Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
    }

    pub fn to_path(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        tracing::debug!(path = %path.as_ref().display(), "Writing keybinds to file");
        serde_json::to_writer_pretty(BufWriter::new(File::create(path)?), self)?;
        Ok(())
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
                ('t'.into(), Action::TuneToView),
                (KeyCode::F(5).into(), Action::Test),
            ]
                .into_iter()
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct Keybind {
    #[serde(rename = "key")]
    code: KeyCode,
    #[serde(
        skip_serializing_if = "KeyModifiers::is_empty",
        default = "KeyModifiers::empty"
    )]
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

mod serde_keybinds {
    use std::fmt;

    use serde::{
        Deserializer,
        Serializer,
        de::Visitor,
        ser::SerializeSeq,
    };

    use super::*;

    #[derive(Serialize, Deserialize)]
    struct Pair<K, A> {
        #[serde(flatten)]
        keybind: K,
        action: A,
    }

    pub fn serialize<S>(
        keybinds: &HashMap<Keybind, Action>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut seq = serializer.serialize_seq(Some(keybinds.len()))?;
        for (keybind, action) in keybinds {
            seq.serialize_element(&Pair { keybind, action })?;
        }
        seq.end()
    }

    struct KeybindsVisitor;

    impl<'de> Visitor<'de> for KeybindsVisitor {
        type Value = HashMap<Keybind, Action>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a list of keybinds")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut keybinds = HashMap::with_capacity(seq.size_hint().unwrap_or_default());

            while let Some(pair) = seq.next_element::<Pair<Keybind, Action>>()? {
                keybinds.insert(pair.keybind, pair.action);
            }

            Ok(keybinds)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<HashMap<Keybind, Action>, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(KeybindsVisitor)
    }
}
