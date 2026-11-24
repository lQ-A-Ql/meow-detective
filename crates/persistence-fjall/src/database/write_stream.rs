use super::read_view::SharedTransaction;
use super::*;
use fjall::Readable;

pub(crate) struct WriteStream<'a> {
    transaction: SharedTransaction,
    keyspaces: &'a HashMap<KeyspaceName, Keyspace>,
    hasher: Option<Sha256>,
    operation_count: usize,
    action_bytes: usize,
    mode: PersistMode,
    immutable_epoch: &'a AtomicU64,
    immutable_changed: bool,
    immutable_sealed: &'a AtomicBool,
}

impl<'a> WriteStream<'a> {
    pub(super) fn new(
        transaction: SharedTransaction,
        keyspaces: &'a HashMap<KeyspaceName, Keyspace>,
        mode: PersistMode,
        immutable_epoch: &'a AtomicU64,
        immutable_sealed: &'a AtomicBool,
    ) -> Self {
        Self {
            transaction,
            keyspaces,
            hasher: Some(Sha256::new()),
            operation_count: 0,
            action_bytes: 0,
            mode,
            immutable_epoch,
            immutable_changed: false,
            immutable_sealed,
        }
    }

    pub(crate) fn put(&mut self, keyspace: KeyspaceName, key: Vec<u8>, value: Vec<u8>) {
        self.immutable_changed |= immutable_keyspace(keyspace);
        self.action_bytes += key.len() + value.len();
        hash_action(
            self.hasher
                .as_mut()
                .expect("streamed write digest is available"),
            &WriteAction::Put {
                keyspace,
                key: key.clone(),
                value: value.clone(),
            },
        );
        let keyspace = self
            .keyspaces
            .get(&keyspace)
            .expect("configured keyspace is present");
        self.transaction
            .lock()
            .expect("transaction lock is valid")
            .as_mut()
            .expect("transaction is active")
            .insert(keyspace, key, value);
        self.operation_count += 1;
    }

    pub(crate) fn delete(&mut self, keyspace: KeyspaceName, key: Vec<u8>) {
        self.immutable_changed |= immutable_keyspace(keyspace);
        self.action_bytes += key.len();
        hash_action(
            self.hasher
                .as_mut()
                .expect("streamed write digest is available"),
            &WriteAction::Delete {
                keyspace,
                key: key.clone(),
            },
        );
        let keyspace = self
            .keyspaces
            .get(&keyspace)
            .expect("configured keyspace is present");
        self.transaction
            .lock()
            .expect("transaction lock is valid")
            .as_mut()
            .expect("transaction is active")
            .remove(keyspace, key);
        self.operation_count += 1;
    }

    pub(crate) fn get(&self, keyspace: KeyspaceName, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let value = self
            .transaction
            .lock()
            .map_err(|_| FjallStorageError::LockPoisoned)?
            .as_ref()
            .expect("transaction is active")
            .get(self.keyspace(keyspace), key)
            .map(|value| value.map(|bytes| bytes.to_vec()))
            .map_err(FjallStorageError::from)?;
        if let Some(bytes) = &value {
            record_point_read(bytes.len());
        }
        Ok(value)
    }

    fn keyspace(&self, name: KeyspaceName) -> &Keyspace {
        self.keyspaces
            .get(&name)
            .expect("configured keyspace is present")
    }

    pub(super) fn finish(mut self) -> Result<WriteBatchReceipt> {
        if self.immutable_changed && self.immutable_sealed.load(Ordering::Acquire) {
            return Err(FjallStorageError::InvalidEnvelope(
                "verified generation is immutable".into(),
            ));
        }
        let digest: [u8; 32] = self
            .hasher
            .take()
            .expect("streamed write digest is available")
            .finalize()
            .into();
        let mut transaction = self
            .transaction
            .lock()
            .map_err(|_| FjallStorageError::LockPoisoned)?
            .take()
            .expect("transaction is active");
        if self.operation_count > 0 {
            transaction.insert(
                self.keyspaces
                    .get(&KeyspaceName::Batches)
                    .expect("configured keyspace is present"),
                [b"batch/".as_slice(), digest.as_slice()].concat(),
                self.operation_count.to_be_bytes().to_vec(),
            );
        }
        let operation_count = self.operation_count;
        transaction
            .commit()?
            .map_err(|_| FjallStorageError::WriteConflict)?;
        if self.immutable_changed {
            self.immutable_epoch.fetch_add(1, Ordering::Release);
        }
        if operation_count > 0 {
            record_write_batch(
                operation_count,
                self.action_bytes,
                matches!(self.mode, PersistMode::SyncAll),
            );
        }
        Ok(WriteBatchReceipt {
            operation_count,
            batch_digest: digest,
        })
    }
}
