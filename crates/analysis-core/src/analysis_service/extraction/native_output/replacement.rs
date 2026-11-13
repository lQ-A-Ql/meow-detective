use super::{
    clone_error, AnalysisServiceError, ArtifactRecordStore, PersistMode, SourceStore,
    TimelineRecordStore,
};
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::extraction::replacement_index::ReplacementIndex;
use crate::analysis_service::extraction::state::AnalysisOutputReplacement;
use persistence_source::fjall::ScanDirection;
use std::{collections::BTreeSet, sync::atomic::AtomicBool};

pub(super) fn delete_replacements(
    store: &SourceStore,
    source: &str,
    replacements: &[AnalysisOutputReplacement],
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<(), AnalysisServiceError> {
    let index = ReplacementIndex::new(replacements);
    let artifacts = ArtifactRecordStore::from_source_store(store.clone());
    let timeline = TimelineRecordStore::from_source_store(store.clone());
    let mut processed = 0;
    progress(processed);
    for object in replacements
        .iter()
        .map(|value| value.source_object_id.as_str())
        .collect::<BTreeSet<_>>()
    {
        ensure_not_cancelled(cancel)?;
        let snapshot = artifacts.read_snapshot();
        let mut cursor = None;
        loop {
            ensure_not_cancelled(cancel)?;
            let page = artifacts
                .list_by_source_object(
                    &snapshot,
                    source,
                    object,
                    cursor.as_deref(),
                    ScanDirection::Forward,
                    512,
                )
                .map_err(clone_error)?;
            processed += page.records.len() as u64;
            let ids = page
                .records
                .into_iter()
                .filter(|record| index.matches_artifact(record, source))
                .map(|record| record.id)
                .collect::<Vec<_>>();
            artifacts
                .delete_batch(source, &ids, PersistMode::Buffer)
                .map_err(clone_error)?;
            progress(processed);
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
        let snapshot = timeline.read_snapshot();
        let mut cursor = None;
        loop {
            ensure_not_cancelled(cancel)?;
            let page = timeline
                .list_by_source_object(
                    &snapshot,
                    source,
                    object,
                    cursor.as_deref(),
                    ScanDirection::Forward,
                    512,
                )
                .map_err(clone_error)?;
            processed += page.records.len() as u64;
            let ids = page
                .records
                .into_iter()
                .filter(|record| index.matches_timeline(record))
                .map(|record| record.id)
                .collect::<Vec<_>>();
            timeline
                .delete_batch(source, &ids, PersistMode::Buffer)
                .map_err(clone_error)?;
            progress(processed);
            cursor = page.next_cursor;
            if cursor.is_none() {
                break;
            }
        }
    }
    ensure_not_cancelled(cancel)
}
