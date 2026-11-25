//! Read-side gate for the timeline projection state marker and the ReadWrite
//! deferred marker write. The marker family is `timeline_projection_state`;
//! each published source must carry an explicit projection state.

use super::{ReadOnlySourceStorageHandle, ReadWriteSourceStorageHandle, StorageRuntimeError};
use persistence_fjall::{
    decode_timeline_projection_state, timeline_projection_marker_envelope,
    timeline_projection_state_key, KeyspaceName, PersistMode, SourceReadSnapshot,
    TimelineProjectionStatePayload,
};

impl ReadOnlySourceStorageHandle {
    /// Read the current projection marker.
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

    /// Admit reads only for an explicitly completed projection.
    pub fn require_timeline_projection_complete(
        &self,
        snapshot: &SourceReadSnapshot,
        source_id: &str,
    ) -> Result<(), StorageRuntimeError> {
        let payload = self
            .timeline_projection_state(snapshot, source_id)?
            .ok_or_else(|| StorageRuntimeError::TimelineProjectionNotReady {
                source_id: source_id.to_owned(),
                status: "missing".to_owned(),
                reason: Some("timeline projection state is missing".to_owned()),
            })?;
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
