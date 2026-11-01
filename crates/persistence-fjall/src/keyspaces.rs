use crate::errors::{FjallStorageError, Result};
use fjall::{KeyspaceCreateOptions, SingleWriterTxKeyspace};

pub const KEYSPACE_NAMES: [&str; 15] = [
    "meta",
    "catalog_records",
    "artifact_records",
    "timeline_records",
    "parser_records",
    "catalog_indexes",
    "artifact_indexes",
    "timeline_indexes",
    "parser_indexes",
    "batches",
    "tombstones",
    "partition_records",
    "partition_indexes",
    "recovery_records",
    "recovery_indexes",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyspaceName {
    Meta,
    CatalogRecords,
    ArtifactRecords,
    TimelineRecords,
    ParserRecords,
    CatalogIndexes,
    ArtifactIndexes,
    TimelineIndexes,
    ParserIndexes,
    Batches,
    Tombstones,
    PartitionRecords,
    PartitionIndexes,
    RecoveryRecords,
    RecoveryIndexes,
}

impl KeyspaceName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Meta => "meta",
            Self::CatalogRecords => "catalog_records",
            Self::ArtifactRecords => "artifact_records",
            Self::TimelineRecords => "timeline_records",
            Self::ParserRecords => "parser_records",
            Self::CatalogIndexes => "catalog_indexes",
            Self::ArtifactIndexes => "artifact_indexes",
            Self::TimelineIndexes => "timeline_indexes",
            Self::ParserIndexes => "parser_indexes",
            Self::Batches => "batches",
            Self::Tombstones => "tombstones",
            Self::PartitionRecords => "partition_records",
            Self::PartitionIndexes => "partition_indexes",
            Self::RecoveryRecords => "recovery_records",
            Self::RecoveryIndexes => "recovery_indexes",
        }
    }
}

pub(crate) fn open_keyspaces(
    database: &fjall::SingleWriterTxDatabase,
) -> Result<Vec<(KeyspaceName, SingleWriterTxKeyspace)>> {
    KEYSPACE_NAMES
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let keyspace = database.keyspace(name, KeyspaceCreateOptions::default)?;
            let keyspace_name = match index {
                0 => KeyspaceName::Meta,
                1 => KeyspaceName::CatalogRecords,
                2 => KeyspaceName::ArtifactRecords,
                3 => KeyspaceName::TimelineRecords,
                4 => KeyspaceName::ParserRecords,
                5 => KeyspaceName::CatalogIndexes,
                6 => KeyspaceName::ArtifactIndexes,
                7 => KeyspaceName::TimelineIndexes,
                8 => KeyspaceName::ParserIndexes,
                9 => KeyspaceName::Batches,
                10 => KeyspaceName::Tombstones,
                11 => KeyspaceName::PartitionRecords,
                12 => KeyspaceName::PartitionIndexes,
                13 => KeyspaceName::RecoveryRecords,
                14 => KeyspaceName::RecoveryIndexes,
                _ => return Err(FjallStorageError::UnknownKeyspace((*name).to_owned())),
            };
            Ok((keyspace_name, keyspace))
        })
        .collect()
}
