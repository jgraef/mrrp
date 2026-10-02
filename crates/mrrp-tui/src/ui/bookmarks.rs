use std::{
    fs::File,
    io::{
        BufReader,
        BufWriter,
    },
    path::{
        Path,
        PathBuf,
    },
};

use serde::{
    Deserialize,
    Serialize,
};
use walkdir::WalkDir;

pub use self::import_sdrpp::import_sdrpp_bookmarks;
use crate::Error;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub name: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    pub frequency: u32,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bandwidth: Option<u32>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
}

impl Bookmark {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, Error> {
        Ok(serde_json::from_reader(BufReader::new(File::open(path)?))?)
    }

    pub fn to_path(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        serde_json::to_writer_pretty(BufWriter::new(File::create(path)?), self)?;
        Ok(())
    }
}

#[derive(Debug)]
struct Item {
    #[allow(unused)]
    path: PathBuf,
    bookmark: Bookmark,
}

#[derive(Debug)]
pub struct Bookmarks {
    root: PathBuf,
    items: Vec<Item>,
}

impl Bookmarks {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path = path.as_ref();

        let mut items = vec![];
        for result in WalkDir::new(path) {
            let dir_entry = result?;

            match Bookmark::from_path(dir_entry.path()) {
                Ok(bookmark) => {
                    items.push(Item {
                        path: dir_entry.path().to_owned(),
                        bookmark,
                    })
                }
                Err(_error) => {
                    // just skip
                }
            }
        }

        Ok(Self {
            root: path.to_owned(),
            items,
        })
    }

    pub fn add_and_save_bookmark(&mut self, bookmark: Bookmark) -> Result<(), Error> {
        let path = self.root.join(format!("{}.json", bookmark.name));
        bookmark.to_path(&path)?;
        self.items.push(Item { path, bookmark });
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &Bookmark> {
        self.items.iter().map(|item| &item.bookmark)
    }
}

mod import_sdrpp {
    use std::{
        collections::HashMap,
        fs::File,
        io::BufReader,
        path::Path,
    };

    use serde::Deserialize;

    use crate::Error;

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct FrequencyManagerConfig {
        lists: HashMap<String, List>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct List {
        bookmarks: HashMap<String, Bookmark>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Bookmark {
        bandwidth: f32,
        frequency: f32,
        mode: i32,
    }

    pub fn import_sdrpp_bookmarks(path: impl AsRef<Path>) -> Result<Vec<super::Bookmark>, Error> {
        let config: FrequencyManagerConfig =
            serde_json::from_reader(BufReader::new(File::open(path)?))?;
        let mut bookmarks = vec![];

        for (_, list) in config.lists {
            for (name, bookmark) in list.bookmarks {
                bookmarks.push(super::Bookmark {
                    name,
                    description: None,
                    tags: vec![],
                    frequency: bookmark.frequency as u32,
                    bandwidth: Some(bookmark.bandwidth as u32),
                    mode: convert_mode(bookmark.mode),
                });
            }
        }

        Ok(bookmarks)
    }

    fn convert_mode(mode: i32) -> Option<String> {
        const MODE_NAMES: &'static [&'static str] =
            &["NFM", "WFM", "AM", "DSB", "USB", "CW", "LSB", "RAW"];
        let mode_name = *MODE_NAMES.get(usize::try_from(mode).ok()?)?;
        Some(mode_name.to_owned())
    }
}
