//! A cache of compiled functions. Storage goes through the platform layer.

use std::collections::BTreeMap;

use crate::platform::fs::Storage;

pub struct Cache<S: Storage> {
    storage: S,
    index: BTreeMap<String, usize>,
}

impl<S: Storage> Cache<S> {
    pub fn new(storage: S) -> Self {
        Cache { storage, index: BTreeMap::new() }
    }

    pub fn put(&mut self, key: &str, bytes: &[u8]) {
        self.storage.write(key, bytes);
        self.index.insert(key.to_owned(), bytes.len());
    }
}
