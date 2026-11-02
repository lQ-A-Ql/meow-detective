use crate::{
    database::SourceReadSnapshot,
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
};
use fjall::Readable;
use std::ops::{Bound, RangeBounds};

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
        if let Some(cursor) = cursor {
            if !cursor.starts_with(prefix) {
                return Err(FjallStorageError::InvalidEnvelope(
                    "keyset cursor does not match the requested prefix".to_owned(),
                ));
            }
        }
        let iter = self.snapshot.prefix(self.keyspace(keyspace), prefix);
        let mut entries = Vec::with_capacity(limit);
        let iter = match direction {
            ScanDirection::Forward => Box::new(iter) as Box<dyn Iterator<Item = fjall::Guard>>,
            ScanDirection::Reverse => Box::new(iter.rev()),
        };
        for guard in iter {
            let (key, value) = guard.into_inner()?;
            let eligible = cursor.is_none_or(|cursor| match direction {
                ScanDirection::Forward => key.as_ref() > cursor,
                ScanDirection::Reverse => key.as_ref() < cursor,
            });
            if !eligible {
                continue;
            }
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
