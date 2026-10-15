use super::{
    directory_reference_sequence_matches,
    mft_directory_index_backfill_actions_with_representatives, MftCatalog,
};
use crate::import_pipeline::profile::{Counters, PhaseRecorder};
use crate::parallel_enum::batch_sink::prepare_ntfs_index_insert;
use crate::parallel_enum::error::ParallelEnumError;
use evidence_core::EvidenceReader;
use fs_ntfs::NtfsReader;
use rusqlite::Connection;
use std::collections::{HashSet, VecDeque};
use transport::dto::import_benchmark::ImportBenchmarkPhaseDto as Phase;

impl MftCatalog {
    pub(in crate::parallel_enum) fn backfill_directory_indexes(
        &mut self,
        conn: &Connection,
        data_source_id: &str,
        evidence_reader: Box<dyn EvidenceReader>,
        partition_index: usize,
        volume_offset: u64,
        profile: &mut PhaseRecorder,
    ) -> Result<(), ParallelEnumError> {
        let ntfs = profile
            .measure(Phase::MftDirReaderBuild, || {
                NtfsReader::open(evidence_reader, volume_offset)
            })
            .map_err(|error| {
                ParallelEnumError::MftParams(format!(
                    "Open NTFS reader for directory indexes: {error}"
                ))
            })?;
        profile.count(
            Phase::MftDirReaderBuild,
            Counters {
                builds: 1,
                ..Default::default()
            },
        );
        profile.measure(Phase::MftSizeReconcile, || {
            self.external_file_sizes.reconcile(
                conn,
                &ntfs,
                data_source_id,
                partition_index,
                &mut self.stats,
            )
        })?;
        profile.measure(Phase::MftDirBackfill, || {
            self.backfill_prepared(conn, data_source_id, &ntfs, partition_index)
        })
    }

    fn backfill_prepared(
        &mut self,
        conn: &Connection,
        data_source_id: &str,
        ntfs: &NtfsReader,
        partition_index: usize,
    ) -> Result<(), ParallelEnumError> {
        let mut statement =
            prepare_ntfs_index_insert(conn).map_err(ParallelEnumError::MftParams)?;
        let mut queue = VecDeque::from([5]);
        let mut visited = HashSet::new();
        while let Some(directory_ref) = queue.pop_front() {
            if !visited.insert(directory_ref) {
                continue;
            }
            let entries = match ntfs.list_directory_entries_with_sequence_by_inode(directory_ref) {
                Ok(entries) => entries,
                Err(error) => {
                    self.stats.directory_index_failures =
                        self.stats.directory_index_failures.saturating_add(1);
                    tracing::warn!(
                        inode = directory_ref,
                        %error,
                        "Failed to list NTFS directory index during MFT catalog backfill"
                    );
                    continue;
                }
            };
            let entries = entries
                .into_iter()
                .filter_map(|(entry, sequence)| {
                    directory_reference_sequence_matches(
                        &self.record_sequences,
                        entry.mft_ref,
                        sequence,
                    )
                    .then_some(entry)
                })
                .collect();
            let actions = mft_directory_index_backfill_actions_with_representatives(
                &mut self.path_map,
                &mut self.index_representatives,
                directory_ref,
                entries,
            );
            for action in actions {
                self.insert_backfill_action(
                    &mut statement,
                    data_source_id,
                    partition_index,
                    directory_ref,
                    &action,
                )?;
                if action.is_dir && !visited.contains(&action.mft_ref) {
                    queue.push_back(action.mft_ref);
                }
            }
        }
        Ok(())
    }
}
