use super::*;
use std::{collections::BTreeMap, time::Instant};

impl SourceWriteSession {
    pub(super) fn ingest(
        &self,
        worker: usize,
        actions: Vec<WriteAction>,
        cancel: &AtomicBool,
    ) -> Result<()> {
        self.ensure_active(cancel)?;
        let _permit = self
            .database
            .inner
            .lifecycle
            .read()
            .map_err(|_| FjallStorageError::LockPoisoned)?;
        let bytes = actions.iter().map(action_bytes).sum::<usize>();
        if bytes > self.config.batch_bytes {
            return Err(FjallStorageError::WriteBatchLimit {
                bytes,
                limit: self.config.batch_bytes,
            });
        }
        let immutable = actions
            .iter()
            .any(|action| immutable_keyspace(action_keyspace(action)));
        if immutable && self.database.inner.immutable_sealed.load(Ordering::Acquire) {
            return Err(FjallStorageError::InvalidEnvelope(
                "verified generation is immutable".into(),
            ));
        }
        let mut hasher = Sha256::new();
        let count = actions.len();
        let database = &self.database.inner;
        let mut grouped = BTreeMap::<KeyspaceName, BTreeMap<Vec<u8>, Option<Vec<u8>>>>::new();
        for action in actions {
            hash_action(&mut hasher, &action);
            match action {
                WriteAction::Put {
                    keyspace,
                    key,
                    value,
                } => {
                    grouped
                        .entry(keyspace)
                        .or_default()
                        .insert(key, Some(value));
                }
                WriteAction::Delete { keyspace, key } => {
                    grouped.entry(keyspace).or_default().insert(key, None);
                }
            }
        }
        for (name, entries) in grouped {
            self.ensure_active(cancel)?;
            let keyspace = database.keyspaces.get(&name).expect("configured keyspace");
            let mut ingestion = keyspace.start_ingestion()?;
            for (key, value) in entries {
                self.ensure_active(cancel)?;
                let _call = self.metrics.enter(worker);
                match value {
                    Some(value) => ingestion.write(key, value)?,
                    None => ingestion.write_tombstone(key)?,
                }
            }
            let started = Instant::now();
            ingestion.finish()?;
            self.metrics.finish_micros.fetch_add(
                started.elapsed().as_micros().try_into().unwrap_or(u64::MAX),
                Ordering::Relaxed,
            );
        }
        let digest: [u8; 32] = hasher.finalize().into();
        let mut receipt = database.engine.inner().batch();
        receipt.insert(
            self.database
                .inner
                .keyspaces
                .get(&KeyspaceName::Batches)
                .expect("configured keyspace"),
            [b"batch/".as_slice(), digest.as_slice()].concat(),
            count.to_be_bytes().to_vec(),
        );
        receipt.commit()?;
        if immutable {
            self.database
                .inner
                .immutable_epoch
                .fetch_add(1, Ordering::Release);
        }
        self.metrics.batches.fetch_add(1, Ordering::Relaxed);
        self.metrics
            .actions
            .fetch_add(count as u64, Ordering::Relaxed);
        self.metrics
            .bytes
            .fetch_add(bytes as u64, Ordering::Relaxed);
        record_write_batch(count, bytes, false);
        Ok(())
    }
}

fn action_keyspace(action: &WriteAction) -> KeyspaceName {
    match action {
        WriteAction::Put { keyspace, .. } | WriteAction::Delete { keyspace, .. } => *keyspace,
    }
}

fn action_bytes(action: &WriteAction) -> usize {
    match action {
        WriteAction::Put { key, value, .. } => key.len() + value.len(),
        WriteAction::Delete { key, .. } => key.len(),
    }
}
