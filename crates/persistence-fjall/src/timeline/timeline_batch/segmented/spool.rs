//! Validated, bounded-memory staging using serde's JSON stream codec.

use super::{validate_replacement_record, SegmentLimits};
use crate::{FjallStorageError, TimelineRecord, TimelineRecordStore, TimelineStreamError};
use serde::{Deserialize, Serialize};
use std::io::{BufReader, BufWriter, Seek, Write};

#[derive(Serialize, Deserialize)]
pub(super) struct StagedRecord {
    pub record: TimelineRecord,
    pub write_bytes: u64,
}

pub(super) struct RecordSpool {
    file: tempfile::NamedTempFile,
    pub records: u64,
}

impl RecordSpool {
    pub(super) fn stage<E, F>(
        store: &TimelineRecordStore,
        source: &str,
        prefix: &str,
        limits: &SegmentLimits,
        mut producer: F,
    ) -> Result<Self, TimelineStreamError<E>>
    where
        E: std::error::Error + 'static,
        F: FnMut() -> Result<Option<Vec<TimelineRecord>>, E>,
    {
        let mut file = tempfile::NamedTempFile::new_in(store.source_store().database().path())
            .map_err(FjallStorageError::from)
            .map_err(TimelineStreamError::Storage)?;
        let mut records = 0u64;
        {
            let mut writer = BufWriter::new(file.as_file_mut());
            while let Some(batch) = producer().map_err(TimelineStreamError::Producer)? {
                if batch.is_empty() {
                    return Err(TimelineStreamError::Storage(
                        FjallStorageError::InvalidEnvelope(
                            "timeline producer returned an empty batch".into(),
                        ),
                    ));
                }
                for record in batch {
                    validate_replacement_record(&record, source, prefix)
                        .map_err(TimelineStreamError::Storage)?;
                    let write_bytes = TimelineRecordStore::record_storage_cost(&record)
                        .map_err(TimelineStreamError::Storage)?
                        .write_bytes;
                    if write_bytes > limits.max_bytes {
                        return Err(TimelineStreamError::Storage(
                            FjallStorageError::InvalidEnvelope(
                                "timeline record exceeds the segment byte limit".into(),
                            ),
                        ));
                    }
                    serde_json::to_writer(
                        &mut writer,
                        &StagedRecord {
                            record,
                            write_bytes,
                        },
                    )
                    .map_err(|error| {
                        TimelineStreamError::Storage(FjallStorageError::Serialization(
                            error.to_string(),
                        ))
                    })?;
                    writer
                        .write_all(b"\n")
                        .map_err(FjallStorageError::from)
                        .map_err(TimelineStreamError::Storage)?;
                    records = records.checked_add(1).ok_or_else(|| {
                        TimelineStreamError::Storage(FjallStorageError::InvalidEnvelope(
                            "timeline record count overflow".into(),
                        ))
                    })?;
                }
            }
            writer
                .flush()
                .map_err(FjallStorageError::from)
                .map_err(TimelineStreamError::Storage)?;
        }
        file.as_file_mut()
            .rewind()
            .map_err(FjallStorageError::from)
            .map_err(TimelineStreamError::Storage)?;
        Ok(Self { file, records })
    }

    pub(super) fn reader(
        &mut self,
    ) -> impl Iterator<Item = Result<StagedRecord, FjallStorageError>> + '_ {
        serde_json::Deserializer::from_reader(BufReader::new(self.file.as_file_mut()))
            .into_iter::<StagedRecord>()
            .map(|record| {
                record.map_err(|error| FjallStorageError::Deserialization(error.to_string()))
            })
    }
}
