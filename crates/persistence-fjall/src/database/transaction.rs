use super::*;
use std::sync::Mutex;

impl SourceDatabase {
    pub(crate) fn write_transaction(
        &self,
        mode: PersistMode,
        plan: impl FnOnce(&TransactionReadView) -> Result<Vec<WriteAction>>,
    ) -> Result<WriteBatchReceipt> {
        let (_, receipt) = self.write_streamed(mode, |view, writer| {
            for action in plan(view)? {
                match action {
                    WriteAction::Put {
                        keyspace,
                        key,
                        value,
                    } => writer.put(keyspace, key, value),
                    WriteAction::Delete { keyspace, key } => writer.delete(keyspace, key),
                }
            }
            Ok::<_, FjallStorageError>(())
        })?;
        Ok(receipt)
    }

    pub(crate) fn write_streamed<T, E>(
        &self,
        mode: PersistMode,
        plan: impl FnOnce(&TransactionReadView, &mut WriteStream<'_>) -> std::result::Result<T, E>,
    ) -> std::result::Result<(T, WriteBatchReceipt), E>
    where
        E: From<FjallStorageError>,
    {
        let _permit = self
            .inner
            .lifecycle
            .read()
            .map_err(|_| E::from(FjallStorageError::LockPoisoned))?;
        self.ensure_regular_write().map_err(E::from)?;
        let transaction = self
            .inner
            .engine
            .write_tx()
            .map_err(FjallStorageError::from)
            .map_err(E::from)?
            .durability(Some(mode.into()));
        let transaction = Arc::new(Mutex::new(Some(transaction)));
        let view = TransactionReadView::new(self, transaction.clone());
        let mut stream = WriteStream::new(
            transaction,
            &self.inner.keyspaces,
            mode,
            &self.inner.immutable_epoch,
            &self.inner.immutable_sealed,
        );
        let value = plan(&view, &mut stream)?;
        drop(view);
        let receipt = stream.finish().map_err(E::from)?;
        Ok((value, receipt))
    }
}
