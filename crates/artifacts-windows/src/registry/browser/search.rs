use super::{BrowserError, BrowserSearchHit, BrowserSearchPage, HiveSnapshot};
use std::collections::HashSet;

const MAX_KEYS: usize = 200_000;
const MAX_HITS: usize = 512;

impl HiveSnapshot {
    /// Bounded depth-first search over loaded immutable hive bytes.
    pub fn search(&self, query: &str) -> Result<BrowserSearchPage, BrowserError> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || query.len() > 256 {
            return Err(BrowserError::InvalidPath);
        }
        let mut stack = vec![self.root_key()?];
        let mut seen = HashSet::new();
        let mut hits = Vec::new();
        let mut scanned_keys = 0u32;
        let mut scanned_values = 0u32;
        let mut skipped = 0u32;
        while let Some(key) = stack.pop() {
            if !seen.insert(key.offset) {
                continue;
            }
            if seen.len() > MAX_KEYS {
                return Err(BrowserError::Limit);
            }
            scanned_keys = scanned_keys.saturating_add(1);
            if key.path.to_lowercase().contains(&query) || key.name.to_lowercase().contains(&query)
            {
                hits.push(BrowserSearchHit {
                    kind: "key",
                    path: key.path.clone(),
                    name: key.name.clone(),
                    cell_offset: key.offset,
                    matched_field: "key",
                    preview: key.path.clone(),
                });
            }
            let values = self.values(&key, 0, key.value_count.min(1024) as usize)?;
            scanned_values = scanned_values.saturating_add(values.declared);
            skipped = skipped.saturating_add(values.skipped);
            for (_, value) in values.items {
                let Ok(value) = value else {
                    skipped = skipped.saturating_add(1);
                    continue;
                };
                if value.name.to_lowercase().contains(&query)
                    || value.preview.to_lowercase().contains(&query)
                {
                    hits.push(BrowserSearchHit {
                        kind: "value",
                        path: key.path.clone(),
                        name: value.name.clone(),
                        cell_offset: value.offset,
                        matched_field: if value.name.to_lowercase().contains(&query) {
                            "valueName"
                        } else {
                            "valueData"
                        },
                        preview: value.preview.clone(),
                    });
                }
                if hits.len() >= MAX_HITS {
                    return Ok(BrowserSearchPage {
                        hits,
                        scanned_keys,
                        scanned_values,
                        skipped,
                        truncated: true,
                    });
                }
            }
            let children = self.subkeys(&key, 0, key.subkey_count.min(1024) as usize)?;
            skipped = skipped.saturating_add(children.skipped);
            for (_, child) in children.items {
                if let Ok(child) = child {
                    stack.push(child);
                } else {
                    skipped = skipped.saturating_add(1);
                }
            }
        }
        Ok(BrowserSearchPage {
            hits,
            scanned_keys,
            scanned_values,
            skipped,
            truncated: false,
        })
    }
}
