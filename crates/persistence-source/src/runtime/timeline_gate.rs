//! Read-side gate for the timeline projection state marker and the ReadWrite
//! deferred marker write. The marker family is `timeline_projection_state`;
//! a missing marker means a legacy store and passes the gate.

use super::{ReadOnlySourceStorageHandle, ReadWriteSourceStorageHandle, StorageRuntimeError};
use persistence_fjall::{
    decode_timeline_projection_state, timeline_projection_marker_envelope,
    timeline_projection_state_key, KeyspaceName, PersistMode, SourceReadSnapshot,
    TimelineProjectionStatePayload,
};

impl ReadOnlySourceStorageHandle {
    /// Read the current projection marker. A legacy store without a marker
    /// returns `None`.
    pub fn timeline_projection_state(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
    ) -> Result<Option<TimelineProjectionStatePayload>, StorageRuntimeError> {
        let key = timeline_projection_state_key(source_id).map_err(StorageRuntimeError::Source)?;
        let Some(envelope) = self.read_envelope(snapshot, KeyspaceName::Meta, &key)? else {
            return Ok(None);
        };
        decode_timeline_projection_state(&envelope).map_err(StorageRuntimeError::Source)
    }

    /// Reject reads while the projection is `Building` or `Deferred`; a
    /// missing marker or `Complete` passes.
    pub fn require_timeline_projection_complete(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
    ) -> Result<(), StorageRuntimeError> {
        let Some(payload) = self.timeline_projection_state(snapshot, source_id)? else {
            return Ok(());
        };
        match payload.status {
            persistence_fjall::TimelineProjectionState::Complete => Ok(()),
            status => Err(StorageRuntimeError::TimelineProjectionNotReady {
                source_id: source_id.to_owned(),
                status: status.as_str().to_owned(),
                reason: payload.reason,
            }),
        }
    }
}

impl ReadWriteSourceStorageHandle {
    /// Write the `Deferred` marker in its own SyncAll transaction.
    pub fn timeline_projection_mark_deferred(
        &self,
        source_id: &str,
        payload: &TimelineProjectionStatePayload,
        mode: PersistMode,
    ) -> Result<(), StorageRuntimeError> {
        let (key, envelope) = timeline_projection_marker_envelope(source_id, payload)
            .map_err(StorageRuntimeError::Source)?;
        self.write_envelope(KeyspaceName::Meta, key, &envelope, mode)?;
        Ok(())
    }
}
