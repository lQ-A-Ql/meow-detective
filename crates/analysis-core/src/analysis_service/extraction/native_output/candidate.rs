use super::{
    clone_error, delete_replacements, AnalysisServiceError, ArtifactRecordStore, PersistMode,
    StreamingGeneration, TimelineRecordStore,
};
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::extraction::replacement_index::ReplacementIndex;
use crate::analysis_service::extraction::state::AnalysisOutputReplacement;
use persistence_source::fjall::ScanDirection;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};

impl StreamingGeneration {
    pub(in crate::analysis_service::extraction) fn complete_replacements(
        &self,
        replacements: &[AnalysisOutputReplacement],
        cancel: &AtomicBool,
    ) -> Result<u64, AnalysisServiceError> {
        self.copy_or_retire_originals(replacements, cancel, false)
    }

    pub(in crate::analysis_service::extraction) fn preserve_replacements(
        &self,
        replacements: &[AnalysisOutputReplacement],
        cancel: &AtomicBool,
    ) -> Result<u64, AnalysisServiceError> {
        delete_replacements(
            &self.store,
            &self.source_id,
            replacements,
            Some(&self.verified_clone),
            cancel,
            |_| {},
        )?;
        self.wrote_artifacts.store(true, Ordering::Relaxed);
        self.wrote_events.store(true, Ordering::Relaxed);
        self.copy_or_retire_originals(replacements, cancel, true)
    }

    fn copy_or_retire_originals(
        &self,
        replacements: &[AnalysisOutputReplacement],
        cancel: &AtomicBool,
        restore: bool,
    ) -> Result<u64, AnalysisServiceError> {
        let index = ReplacementIndex::new(replacements);
        let mut count = 0;
        for object in replacements
            .iter()
            .map(|replacement| replacement.source_object_id.as_str())
            .collect::<BTreeSet<_>>()
        {
            count += self.original_artifacts(object, &index, cancel, restore)?;
            count += self.original_events(object, &index, cancel, restore)?;
        }
        ensure_not_cancelled(cancel)?;
        Ok(count)
    }

    fn original_artifacts(
        &self,
        object: &str,
        index: &ReplacementIndex<'_>,
        cancel: &AtomicBool,
        restore: bool,
    ) -> Result<u64, AnalysisServiceError> {
        let original = ArtifactRecordStore::from_source_store(self.original.clone());
        let target = ArtifactRecordStore::from_source_store(self.store.clone());
        let snapshot = original.read_snapshot();
        let mut cursor = None;
        let mut count = 0;
        loop {
            ensure_not_cancelled(cancel)?;
            let page = original
                .list_by_source_object(
                    &snapshot,
                    &self.source_id,
                    object,
                    cursor.as_deref(),
                    ScanDirection::Forward,
                    512,
                )
                .map_err(clone_error)?;
            let records = page
                .records
                .into_iter()
                .filter(|record| index.matches_artifact(record, &self.source_id))
                .collect::<Vec<_>>();
            count += records.len() as u64;
            if !records.is_empty() {
                if restore {
                    target
                        .upsert_batch_independent(&records, PersistMode::Buffer)
                        .map_err(clone_error)?;
                } else {
                    let ids = records
                        .into_iter()
                        .map(|record| record.id)
                        .collect::<Vec<_>>();
                    target
                        .delete_batch_in_generation(
                            &self.verified_clone,
                            &self.source_id,
                            &ids,
                            PersistMode::Buffer,
                        )
                        .map_err(clone_error)?;
                }
                self.wrote_artifacts.store(true, Ordering::Relaxed);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(count);
            }
        }
    }

    fn original_events(
        &self,
        object: &str,
        index: &ReplacementIndex<'_>,
        cancel: &AtomicBool,
        restore: bool,
    ) -> Result<u64, AnalysisServiceError> {
        let original = TimelineRecordStore::from_source_store(self.original.clone());
        let target = TimelineRecordStore::from_source_store(self.store.clone());
        let snapshot = original.read_snapshot();
        let mut cursor = None;
        let mut count = 0;
        loop {
            ensure_not_cancelled(cancel)?;
            let page = original
                .list_by_source_object(
                    &snapshot,
                    &self.source_id,
                    object,
                    cursor.as_deref(),
                    ScanDirection::Forward,
                    512,
                )
                .map_err(clone_error)?;
            let records = page
                .records
                .into_iter()
                .filter(|record| index.matches_timeline(record))
                .collect::<Vec<_>>();
            count += records.len() as u64;
            if !records.is_empty() {
                if restore {
                    target
                        .upsert_batch_independent(&records, PersistMode::Buffer)
                        .map_err(clone_error)?;
                } else {
                    let ids = records
                        .into_iter()
                        .map(|record| record.id)
                        .collect::<Vec<_>>();
                    target
                        .delete_batch_in_generation(
                            &self.verified_clone,
                            &self.source_id,
                            &ids,
                            PersistMode::Buffer,
                        )
                        .map_err(clone_error)?;
                }
                self.wrote_events.store(true, Ordering::Relaxed);
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Ok(count);
            }
        }
    }
}
