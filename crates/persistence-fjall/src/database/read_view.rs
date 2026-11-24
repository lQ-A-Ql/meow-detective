use super::*;
use fjall::{OptimisticWriteTx, Readable};
use std::{
    ops::{Deref, RangeBounds},
    sync::Mutex,
};

pub(super) type SharedTransaction = Arc<Mutex<Option<OptimisticWriteTx>>>;

pub(crate) enum ReadView {
    Snapshot(fjall::Snapshot),
    Transaction(SharedTransaction),
}

impl ReadView {
    pub(crate) fn get(
        &self,
        keyspace: &Keyspace,
        key: &[u8],
    ) -> fjall::Result<Option<fjall::Slice>> {
        match self {
            Self::Snapshot(snapshot) => snapshot.get(keyspace, key),
            Self::Transaction(transaction) => transaction
                .lock()
                .expect("transaction lock is valid")
                .as_ref()
                .expect("transaction is active")
                .get(keyspace, key),
        }
    }

    pub(crate) fn range<K: AsRef<[u8]>, R: RangeBounds<K>>(
        &self,
        keyspace: &Keyspace,
        bounds: R,
    ) -> fjall::Iter {
        match self {
            Self::Snapshot(snapshot) => snapshot.range(keyspace, bounds),
            Self::Transaction(transaction) => transaction
                .lock()
                .expect("transaction lock is valid")
                .as_ref()
                .expect("transaction is active")
                .range(keyspace, bounds),
        }
    }

    pub(crate) fn prefix(&self, keyspace: &Keyspace, prefix: &[u8]) -> fjall::Iter {
        match self {
            Self::Snapshot(snapshot) => snapshot.prefix(keyspace, prefix),
            Self::Transaction(transaction) => transaction
                .lock()
                .expect("transaction lock is valid")
                .as_ref()
                .expect("transaction is active")
                .prefix(keyspace, prefix),
        }
    }
}

pub(crate) struct TransactionReadView(SourceReadSnapshot);

impl TransactionReadView {
    pub(super) fn new(database: &SourceDatabase, transaction: SharedTransaction) -> Self {
        Self(SourceReadSnapshot {
            snapshot: ReadView::Transaction(transaction),
            keyspaces: database.inner.keyspaces.clone(),
            _owner: database.clone(),
        })
    }
}

impl Deref for TransactionReadView {
    type Target = SourceReadSnapshot;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct SourceReadHandle {
    database: SourceDatabase,
}

impl SourceReadHandle {
    pub fn path(&self) -> &Path {
        self.database.path()
    }
    pub fn read_snapshot(&self) -> SourceReadSnapshot {
        self.database.read_snapshot()
    }
}

impl SourceDatabase {
    pub fn read_handle(&self) -> SourceReadHandle {
        SourceReadHandle {
            database: self.clone(),
        }
    }
}
