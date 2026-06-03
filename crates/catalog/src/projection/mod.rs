use domain::{FileEntry, FileEntryId};
use std::collections::HashMap;

/// Projection that groups file entries by their extension.
#[derive(Debug, Clone, Default)]
pub struct ExtensionProjection {
    index: HashMap<String, Vec<FileEntryId>>,
}

impl ExtensionProjection {
    pub fn build(entries: &[FileEntry]) -> Self {
        let mut index: HashMap<String, Vec<FileEntryId>> = HashMap::new();
        for entry in entries {
            let ext = entry.ext.clone().unwrap_or_default();
            index.entry(ext).or_default().push(entry.id.clone());
        }
        Self { index }
    }

    pub fn query(&self, ext: &str) -> &[FileEntryId] {
        self.index.get(ext).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn extensions(&self) -> Vec<&str> {
        self.index.keys().map(|s| s.as_str()).collect()
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }
}

/// Projection that groups file entries by path prefix.
#[derive(Debug, Clone, Default)]
pub struct PathPrefixProjection {
    index: Vec<(String, Vec<FileEntryId>)>,
}

impl PathPrefixProjection {
    pub fn build(entries: &[FileEntry], prefixes: &[&str]) -> Self {
        let mut buckets: HashMap<String, Vec<FileEntryId>> = HashMap::new();
        for prefix in prefixes {
            buckets.insert((*prefix).to_string(), Vec::new());
        }
        for entry in entries {
            for prefix in prefixes {
                if entry.path.starts_with(prefix) {
                    buckets
                        .entry((*prefix).to_string())
                        .or_default()
                        .push(entry.id.clone());
                }
            }
        }
        let mut index: Vec<(String, Vec<FileEntryId>)> = buckets.into_iter().collect();
        index.sort_by(|a, b| a.0.cmp(&b.0));
        Self { index }
    }

    pub fn query(&self, prefix: &str) -> &[FileEntryId] {
        self.index
            .iter()
            .find(|(k, _)| k == prefix)
            .map(|(_, v)| v.as_slice())
            .unwrap_or(&[])
    }

    pub fn prefixes(&self) -> Vec<&str> {
        self.index.iter().map(|(k, _)| k.as_str()).collect()
    }
}
