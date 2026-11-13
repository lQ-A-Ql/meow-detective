use super::*;
use fjall::SingleWriterWriteTx;

pub(crate) struct WriteStream<'a> {
    transaction: SingleWriterWriteTx<'a>,
    keyspaces: &'a HashMap<KeyspaceName, SingleWriterTxKeyspace>,
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
        transaction: SingleWriterWriteTx<'a>,
        keyspaces: &'a HashMap<KeyspaceName, SingleWriterTxKeyspace>,
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
        self.transaction.insert(keyspace, key, value);
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
        self.transaction.remove(keyspace, key);
        self.operation_count += 1;
    }

    pub(crate) fn get(&self, keyspace: KeyspaceName, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let value = self
            .transaction
            .get(self.keyspace(keyspace), key)
            .map(|value| value.map(|bytes| bytes.to_vec()))
            .map_err(FjallStorageError::from)?;
        if let Some(bytes) = &value {
            record_point_read(bytes.len());
        }
        Ok(value)
    }

    fn keyspace(&self, name: KeyspaceName) -> &SingleWriterTxKeyspace {
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
        if self.operation_count > 0 {
            self.transaction.insert(
                self.keyspaces
                    .get(&KeyspaceName::Batches)
                    .expect("configured keyspace is present"),
                [b"batch/".as_slice(), digest.as_slice()].concat(),
                self.operation_count.to_be_bytes().to_vec(),
            );
        }
        let operation_count = self.operation_count;
        self.transaction.commit()?;
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
