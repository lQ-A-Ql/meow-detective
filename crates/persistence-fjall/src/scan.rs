use crate::{
    database::SourceReadSnapshot,
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
};
use fjall::Readable;
use std::ops::{Bound, RangeBounds};

const MAX_SCAN_PAGE_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanDirection {
    Forward,
    Reverse,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotEntry {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanBounds {
    pub start: Bound<Vec<u8>>,
    pub end: Bound<Vec<u8>>,
}

impl ScanBounds {
    pub fn unbounded() -> Self {
        Self {
            start: Bound::Unbounded,
            end: Bound::Unbounded,
        }
    }

    pub fn inclusive(start: impl Into<Vec<u8>>, end: impl Into<Vec<u8>>) -> Self {
        Self {
            start: Bound::Included(start.into()),
            end: Bound::Included(end.into()),
        }
    }

    pub fn exclusive(start: impl Into<Vec<u8>>, end: impl Into<Vec<u8>>) -> Self {
        Self {
            start: Bound::Excluded(start.into()),
            end: Bound::Excluded(end.into()),
        }
    }

    pub fn from_start(start: impl Into<Vec<u8>>, inclusive: bool) -> Self {
        Self {
            start: if inclusive {
                Bound::Included(start.into())
            } else {
                Bound::Excluded(start.into())
            },
            end: Bound::Unbounded,
        }
    }

    pub fn through_end(end: impl Into<Vec<u8>>, inclusive: bool) -> Self {
        Self {
            start: Bound::Unbounded,
            end: if inclusive {
                Bound::Included(end.into())
            } else {
                Bound::Excluded(end.into())
            },
        }
    }
}

impl From<(Bound<Vec<u8>>, Bound<Vec<u8>>)> for ScanBounds {
    fn from((start, end): (Bound<Vec<u8>>, Bound<Vec<u8>>)) -> Self {
        Self { start, end }
    }
}

impl SourceReadSnapshot {
    pub fn range_keyset(
        &self,
        keyspace: KeyspaceName,
        bounds: ScanBounds,
        cursor: Option<&[u8]>,
        limit: usize,
    ) -> Result<Vec<SnapshotEntry>> {
        if limit == 0 {
            return Ok(Vec::new());
        }
        if limit > MAX_SCAN_PAGE_ENTRIES {
            return Err(FjallStorageError::InvalidEnvelope(format!(
                "scan page limit exceeds {} entries",
                MAX_SCAN_PAGE_ENTRIES
            )));
        }
        let start = cursor
            .map(|cursor| Bound::Excluded(cursor.to_vec()))
            .unwrap_or(bounds.start);
        let iter = self
            .snapshot
            .range(self.keyspace(keyspace), (start, bounds.end));
        let mut entries = Vec::with_capacity(limit);
        for guard in iter {
            let (key, value) = guard.into_inner()?;
            entries.push(SnapshotEntry {
                key: key.to_vec(),
                value: value.to_vec(),
            });
            if entries.len() == limit {
                break;
            }
        }
        Ok(entries)
    }

    pub fn prefix(
        &self,
        keyspace: KeyspaceName,
        prefix: &[u8],
        direction: ScanDirection,
    ) -> Result<Vec<SnapshotEntry>> {
        let keyspace = self.keyspace(keyspace);
        collect(self.snapshot.prefix(keyspace, prefix), direction)
    }

    pub fn scan_prefix(
        &self,
        keyspace: KeyspaceName,
        prefix: &[u8],
        direction: ScanDirection,
    ) -> Result<Vec<SnapshotEntry>> {
        self.prefix(keyspace, prefix, direction)
    }

    pub fn range(
        &self,
        keyspace: KeyspaceName,
        bounds: impl Into<ScanBounds>,
        direction: ScanDirection,
    ) -> Result<Vec<SnapshotEntry>> {
        let bounds = bounds.into();
        let keyspace = self.keyspace(keyspace);
        collect(
            self.snapshot.range(keyspace, (bounds.start, bounds.end)),
            direction,
        )
    }

    pub fn scan_range<R>(
        &self,
        keyspace: KeyspaceName,
        bounds: R,
        direction: ScanDirection,
    ) -> Result<Vec<SnapshotEntry>>
    where
        R: RangeBounds<Vec<u8>>,
    {
        self.range(keyspace, clone_bounds(bounds), direction)
    }

    pub fn keyset(
        &self,
        keyspace: KeyspaceName,
        prefix: &[u8],
        cursor: Option<&[u8]>,
        direction: ScanDirection,
        limit: usize,
    ) -> Result<Vec<SnapshotEntry>> {
        if limit > MAX_SCAN_PAGE_ENTRIES {
            return Err(FjallStorageError::InvalidEnvelope(format!(
                "scan page limit exceeds {} entries",
                MAX_SCAN_PAGE_ENTRIES
            )));
        }
        if let Some(cursor) = cursor {
            if !cursor.starts_with(prefix) {
                return Err(FjallStorageError::InvalidEnvelope(
                    "keyset cursor does not match the requested prefix".to_owned(),
                ));
            }
        }
        let prefix_end = prefix_end(prefix).ok_or_else(|| {
            FjallStorageError::InvalidEnvelope("keyset prefix has no upper bound".to_owned())
        })?;
        let bounds = match (direction, cursor) {
            (ScanDirection::Forward, Some(cursor)) => ScanBounds {
                start: Bound::Excluded(cursor.to_vec()),
                end: Bound::Excluded(prefix_end),
            },
            (ScanDirection::Forward, None) => ScanBounds {
                start: Bound::Included(prefix.to_vec()),
                end: Bound::Excluded(prefix_end),
            },
            (ScanDirection::Reverse, Some(cursor)) => ScanBounds {
                start: Bound::Included(prefix.to_vec()),
                end: Bound::Excluded(cursor.to_vec()),
            },
            (ScanDirection::Reverse, None) => ScanBounds {
                start: Bound::Included(prefix.to_vec()),
                end: Bound::Excluded(prefix_end),
            },
        };
        let iter = self
            .snapshot
            .range(self.keyspace(keyspace), (bounds.start, bounds.end));
        let mut entries = Vec::with_capacity(limit);
        let iter = match direction {
            ScanDirection::Forward => Box::new(iter) as Box<dyn Iterator<Item = fjall::Guard>>,
            ScanDirection::Reverse => Box::new(iter.rev()),
        };
        for guard in iter {
            let (key, value) = guard.into_inner()?;
            entries.push(SnapshotEntry {
                key: key.to_vec(),
                value: value.to_vec(),
            });
            if entries.len() >= limit {
                break;
            }
        }
        Ok(entries)
    }

    fn keyspace(&self, keyspace: KeyspaceName) -> &fjall::SingleWriterTxKeyspace {
        self.keyspaces
            .get(&keyspace)
            .expect("configured keyspace is present")
    }
}

fn prefix_end(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.last_mut() {
        if *last == u8::MAX {
            end.pop();
        } else {
            *last += 1;
            return Some(end);
        }
    }
    None
}

fn clone_bounds<R>(bounds: R) -> ScanBounds
where
    R: RangeBounds<Vec<u8>>,
{
    ScanBounds {
        start: clone_bound(bounds.start_bound()),
        end: clone_bound(bounds.end_bound()),
    }
}

fn clone_bound(bound: Bound<&Vec<u8>>) -> Bound<Vec<u8>> {
    match bound {
        Bound::Included(value) => Bound::Included(value.clone()),
        Bound::Excluded(value) => Bound::Excluded(value.clone()),
        Bound::Unbounded => Bound::Unbounded,
    }
}

fn collect(iter: fjall::Iter, direction: ScanDirection) -> Result<Vec<SnapshotEntry>> {
    let iter = match direction {
        ScanDirection::Forward => Box::new(iter) as Box<dyn Iterator<Item = fjall::Guard>>,
        ScanDirection::Reverse => Box::new(iter.rev()),
    };
    iter.map(|guard| {
        let (key, value) = guard.into_inner()?;
        Ok(SnapshotEntry {
            key: key.to_vec(),
            value: value.to_vec(),
        })
    })
    .collect()
}
