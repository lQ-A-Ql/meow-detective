use super::{
    cells::{array, name, u16_at, u32_at},
    BrowserError, BrowserKey, BrowserPage, HiveSnapshot,
};
use std::collections::HashSet;

const MAX_LIST_ITEMS: usize = 100_000;

impl HiveSnapshot {
    pub fn root_key(&self) -> Result<BrowserKey, BrowserError> {
        self.key(self.root, String::new())
    }

    pub fn key(&self, offset: u32, path: String) -> Result<BrowserKey, BrowserError> {
        let cell = self.cell(offset)?;
        if cell.get(..2) != Some(b"nk") || cell.len() < 76 {
            return Err(BrowserError::Corrupt);
        }
        let length = u16_at(cell, 72)? as usize;
        let bytes = cell.get(76..76 + length).ok_or(BrowserError::Corrupt)?;
        let filetime = u64::from_le_bytes(array(cell, 4)?);
        Ok(BrowserKey {
            offset,
            name: name(bytes, u16_at(cell, 2)? & 0x20 != 0)?,
            path,
            filetime: (filetime != 0).then_some(filetime),
            subkey_count: u32_at(cell, 20)?,
            subkeys: u32_at(cell, 28)?,
            value_count: u32_at(cell, 36)?,
            values: u32_at(cell, 40)?,
        })
    }

    pub fn resolve(&self, path: &str) -> Result<BrowserKey, BrowserError> {
        if path.len() > 4096 || path.contains('\0') || path.contains('/') {
            return Err(BrowserError::InvalidPath);
        }
        let parts: Vec<_> = path
            .trim_matches('\\')
            .split('\\')
            .filter(|v| !v.is_empty())
            .collect();
        if parts.len() > 64 {
            return Err(BrowserError::Limit);
        }
        let mut key = self.root_key()?;
        let mut seen = HashSet::from([key.offset]);
        for part in parts {
            let (offsets, _) = self.key_offsets(&key)?;
            let mut found = None;
            for offset in offsets {
                if let Ok(child) = self.key(offset, String::new()) {
                    if child.name.to_lowercase() == part.to_lowercase() {
                        found = Some(child);
                        break;
                    }
                }
            }
            let mut child = found.ok_or(BrowserError::NotFound)?;
            if !seen.insert(child.offset) {
                return Err(BrowserError::Corrupt);
            }
            child.path = join(&key.path, &child.name);
            key = child;
        }
        Ok(key)
    }

    pub fn subkeys(
        &self,
        key: &BrowserKey,
        start: usize,
        limit: usize,
    ) -> Result<BrowserPage<BrowserKey>, BrowserError> {
        let (offsets, diagnostic) = self.key_offsets(key)?;
        let end = start
            .checked_add(limit)
            .ok_or(BrowserError::Limit)?
            .min(offsets.len());
        if start > offsets.len() {
            return Err(BrowserError::Limit);
        }
        let items = offsets[start..end]
            .iter()
            .map(|offset| {
                let item = self.key(*offset, String::new()).map(|mut child| {
                    child.path = join(&key.path, &child.name);
                    child
                });
                (*offset, item)
            })
            .collect();
        Ok(BrowserPage {
            declared: key.subkey_count,
            items,
            next: (end < offsets.len()).then_some(end),
            diagnostic,
        })
    }

    fn key_offsets(
        &self,
        key: &BrowserKey,
    ) -> Result<(Vec<u32>, Option<BrowserError>), BrowserError> {
        if key.subkey_count == 0 {
            return Ok((Vec::new(), None));
        }
        if key.subkey_count as usize > MAX_LIST_ITEMS {
            return Err(BrowserError::Limit);
        }
        let mut offsets = Vec::new();
        let mut seen = HashSet::new();
        let error = self
            .walk_list(key.subkeys, 0, &mut seen, &mut offsets)
            .err();
        let mismatch = offsets.len() != key.subkey_count as usize;
        Ok((offsets, error.or(mismatch.then_some(BrowserError::Corrupt))))
    }

    fn walk_list(
        &self,
        offset: u32,
        depth: u8,
        seen: &mut HashSet<u32>,
        out: &mut Vec<u32>,
    ) -> Result<(), BrowserError> {
        if depth > 8 || !seen.insert(offset) {
            return Err(BrowserError::Corrupt);
        }
        let cell = self.cell(offset)?;
        let signature = cell.get(..2).ok_or(BrowserError::Corrupt)?;
        let count = u16_at(cell, 2)? as usize;
        let stride = match signature {
            b"lf" | b"lh" => 8,
            b"li" | b"ri" => 4,
            _ => return Err(BrowserError::Corrupt),
        };
        if count * stride + 4 > cell.len() {
            return Err(BrowserError::Corrupt);
        }
        for index in 0..count {
            let child = u32_at(cell, 4 + index * stride)?;
            if signature == b"ri" {
                self.walk_list(child, depth + 1, seen, out)?;
            } else {
                out.push(child);
            }
            if out.len() > MAX_LIST_ITEMS {
                return Err(BrowserError::Limit);
            }
        }
        Ok(())
    }
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}\\{name}")
    }
}
