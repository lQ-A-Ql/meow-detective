use super::{
    decode, BrowserError, BrowserKey, BrowserSearchHit, BrowserSearchPage, BrowserValue,
    HiveSnapshot,
};
use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};
const MAX_HITS: usize = 512;
const MAX_REFERENCE_BYTES: usize = 32 * 1024 * 1024;
const MAX_DATA_SCAN: u64 = 512 * 1024 * 1024;
pub(super) struct SearchWalk<'a> {
    hive: &'a HiveSnapshot,
    query: String,
    cancel: &'a AtomicBool,
    progress: &'a (dyn Fn(u64, u64) + Sync),
    seen_keys: HashSet<u32>,
    seen_values: HashSet<u32>,
    data_scanned: u64,
    page: BrowserSearchPage,
}
struct Frame {
    key: BrowserKey,
    children: Vec<u32>,
    next: usize,
}
impl<'a> SearchWalk<'a> {
    pub(super) fn run(
        hive: &'a HiveSnapshot,
        query: &str,
        cancel: &'a AtomicBool,
        progress: &'a (dyn Fn(u64, u64) + Sync),
    ) -> Result<BrowserSearchPage, BrowserError> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || query.len() > 256 {
            return Err(BrowserError::InvalidPath);
        }
        let mut walk = Self {
            hive,
            query,
            cancel,
            progress,
            seen_keys: HashSet::new(),
            seen_values: HashSet::new(),
            data_scanned: 0,
            page: BrowserSearchPage {
                hits: Vec::new(),
                scanned_keys: 0,
                scanned_values: 0,
                skipped: u32::from(hive.is_partial()),
                truncated: false,
                total_matches: 0,
                complete: !hive.is_partial(),
            },
        };
        let root = hive.root_key()?;
        let mut stack = vec![walk.enter(root)?];
        while !stack.is_empty() {
            walk.check()?;
            let frame = stack.last_mut().ok_or(BrowserError::Corrupt)?;
            if frame.next >= frame.children.len() {
                stack.pop();
                continue;
            }
            let offset = frame.children[frame.next];
            frame.next += 1;
            let child = hive.key(offset, String::new());
            let Ok(mut child) = child else {
                walk.skip(1);
                continue;
            };
            if child.parent != frame.key.offset || !walk.seen_keys.insert(child.offset) {
                walk.skip(1);
                continue;
            }
            child.path = if frame.key.path.is_empty() {
                child.name.clone()
            } else {
                format!("{}\\{}", frame.key.path, child.name)
            };
            if child.path.len() > 4096 || stack.len() >= 64 {
                walk.skip(1);
                continue;
            }
            let next = walk.enter(child)?;
            if stack
                .iter()
                .map(|frame| frame.children.len() * 4)
                .sum::<usize>()
                + next.children.len() * 4
                > MAX_REFERENCE_BYTES
            {
                walk.skip(1);
                continue;
            }
            stack.push(next);
        }
        walk.page.truncated = walk.page.total_matches > walk.page.hits.len() as u64;
        Ok(walk.page)
    }
    fn enter(&mut self, key: BrowserKey) -> Result<Frame, BrowserError> {
        self.check()?;
        self.seen_keys.insert(key.offset);
        self.page.scanned_keys += 1;
        if key.name.to_lowercase().contains(&self.query)
            || key.path.to_lowercase().contains(&self.query)
        {
            self.hit(BrowserSearchHit {
                kind: "key",
                path: key.path.clone(),
                name: key.name.clone(),
                cell_offset: key.offset,
                matched_field: "key",
                preview: key.path.clone(),
                key_offset: key.offset,
                value_index: None,
                data_offset: None,
                overlay_source: key.overlay_source,
                data_overlay_source: key.overlay_source,
            });
        }
        self.values(&key)?;
        let children = match self.hive.key_offsets(&key) {
            Ok((offsets, diagnostic)) => {
                if diagnostic.is_some() {
                    self.skip(key.subkey_count.saturating_sub(offsets.len() as u32).max(1));
                }
                offsets
            }
            Err(_) => {
                self.skip(key.subkey_count.max(1));
                Vec::new()
            }
        };
        (self.progress)(
            u64::from(self.page.scanned_keys) + u64::from(self.page.scanned_values),
            0,
        );
        Ok(Frame {
            key,
            children,
            next: 0,
        })
    }
    fn values(&mut self, key: &BrowserKey) -> Result<(), BrowserError> {
        let mut start = 0;
        loop {
            self.check()?;
            let page = match self.hive.values(key, start, 256) {
                Ok(page) => page,
                Err(_) => {
                    self.skip(key.value_count.max(1));
                    return Ok(());
                }
            };
            self.skip(page.skipped);
            for (index, (_, value)) in page.items.into_iter().enumerate() {
                self.check()?;
                self.page.scanned_values += 1;
                let Ok(value) = value else {
                    self.skip(1);
                    continue;
                };
                if !self.seen_values.insert(value.offset) {
                    self.skip(1);
                    continue;
                }
                let named = value.name.to_lowercase().contains(&self.query);
                let data = if named {
                    None
                } else {
                    self.data_match(&value)?
                };
                if value.data_error.is_some() {
                    self.skip(1);
                }
                if named || data.is_some() {
                    self.hit(BrowserSearchHit {
                        kind: "value",
                        path: key.path.clone(),
                        name: value.name.clone(),
                        cell_offset: value.offset,
                        matched_field: if named { "valueName" } else { "valueData" },
                        preview: data
                            .as_ref()
                            .map(|(_, preview)| preview.clone())
                            .unwrap_or(value.preview),
                        key_offset: key.offset,
                        value_index: Some((start + index) as u32),
                        data_offset: data.map(|(offset, _)| offset),
                        overlay_source: value.overlay_source,
                        data_overlay_source: value.data_overlay_source,
                    });
                }
            }
            let Some(next) = page.next else { break };
            start = next;
        }
        Ok(())
    }
    fn data_match(&mut self, value: &BrowserValue) -> Result<Option<(u64, String)>, BrowserError> {
        if value.data_error.is_some() {
            return Ok(None);
        }
        let mut at = 0u64;
        let mut tail = Vec::new();
        while at < u64::from(value.byte_length) {
            self.check()?;
            let range = match self.hive.value_range(value, at, 65536) {
                Ok(range) => range,
                Err(_) => {
                    self.skip(1);
                    return Ok(None);
                }
            };
            if range.bytes.is_empty() {
                self.skip(1);
                return Ok(None);
            }
            self.data_scanned += range.bytes.len() as u64;
            if self.data_scanned > MAX_DATA_SCAN {
                self.skip(1);
                return Ok(None);
            }
            let begin = at.saturating_sub(tail.len() as u64);
            tail.extend_from_slice(&range.bytes);
            let partial = begin != 0 || tail.len() != value.byte_length as usize;
            let decoded = decode::decode(value.type_code, &tail, begin as usize, partial)
                .unwrap_or_else(|| hex::encode(&tail));
            if let Some(found) = decoded.to_lowercase().find(&self.query) {
                let mut first = found.saturating_sub(80).min(decoded.len());
                while !decoded.is_char_boundary(first) {
                    first -= 1;
                }
                return Ok(Some((begin, decoded[first..].chars().take(256).collect())));
            }
            at += range.bytes.len() as u64;
            let retained = (self.query.len() * 8 + 8).min(tail.len());
            let retained = retained - retained % 2;
            tail = tail[tail.len() - retained..].to_vec();
        }
        Ok(None)
    }
    fn hit(&mut self, hit: BrowserSearchHit) {
        self.page.total_matches += 1;
        if self.page.hits.len() < MAX_HITS {
            self.page.hits.push(hit);
        }
    }
    fn skip(&mut self, count: u32) {
        if count > 0 {
            self.page.skipped = self.page.skipped.saturating_add(count);
            self.page.complete = false;
        }
    }
    fn check(&self) -> Result<(), BrowserError> {
        if self.cancel.load(Ordering::Acquire) {
            Err(BrowserError::Cancelled)
        } else {
            Ok(())
        }
    }
}
