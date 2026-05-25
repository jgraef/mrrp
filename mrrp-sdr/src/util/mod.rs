pub mod build_info;
pub mod github_urls;

use std::hash::Hash;

use indexmap::IndexSet;
use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Lru<T>
where
    T: Eq + Hash,
{
    limit: Option<usize>,
    items: IndexSet<T>,
}

impl<T> Lru<T>
where
    T: Eq + Hash,
{
    pub fn new(limit: Option<usize>) -> Self {
        Self {
            limit,
            items: IndexSet::with_capacity(limit.unwrap_or_default()),
        }
    }

    pub fn with_limit(limit: usize) -> Self {
        Self::new(Some(limit))
    }

    pub fn insert(&mut self, item: T) -> bool {
        let (inserted_at, newly_inserted) = self.items.insert_full(item);

        let last_index = self.items.len() - 1;

        if inserted_at < last_index {
            self.items.move_index(inserted_at, last_index);
        }

        if newly_inserted && self.limit.is_some_and(|limit| self.items.len() > limit) {
            self.items.shift_remove_index(0);
        }

        newly_inserted
    }

    pub fn iter(&self) -> indexmap::set::Iter<'_, T> {
        self.items.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}
