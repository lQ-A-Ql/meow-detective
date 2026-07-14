use std::path::{Path, PathBuf};

use rusqlite::{params, Connection, OpenFlags};
use transport::CommandError;

mod row;
mod schema;

use self::row::{map_point, map_point_ref, map_range, validate_point, validate_range};
use self::schema::{configure_connection, create_schema};

const SPOOL_SCHEMA_VERSION: u32 = 1;
const MAX_POINT_MUTATIONS: u64 = 5_000_000;
const MAX_RANGE_TOMBSTONES: u64 = 500_000;
const MAX_RESIDENT_RANGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RAW_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SpoolSourceKind {
    Sst,
    Wal,
}

impl SpoolSourceKind {
    pub(super) fn encoded(self) -> u8 {
        match self {
            Self::Sst => 0,
            Self::Wal => 1,
        }
    }

    fn decode(value: u8) -> Result<Self, CommandError> {
        match value {
            0 => Ok(Self::Sst),
            1 => Ok(Self::Wal),
            _ => Err(spool_error("stored mutation has an invalid source kind")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SpoolProvenance {
    pub(super) source_kind: SpoolSourceKind,
    pub(super) file_number: u64,
    pub(super) level: Option<u32>,
    pub(super) physical_offset: u64,
    pub(super) primary_ordinal: u64,
    pub(super) secondary_ordinal: u64,
}

pub(super) struct SpoolPointInput<'a> {
    pub(super) column_family_id: u32,
    pub(super) user_key: &'a [u8],
    pub(super) sequence: u64,
    pub(super) value_type: u8,
    pub(super) value: &'a [u8],
    pub(super) provenance: SpoolProvenance,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SpoolPointRef<'a> {
    pub(super) column_family_id: u32,
    pub(super) user_key: &'a [u8],
    pub(super) sequence: u64,
    pub(super) value_type: u8,
    pub(super) value: &'a [u8],
    pub(super) provenance: SpoolProvenance,
}

pub(super) struct SpoolRangeInput<'a> {
    pub(super) column_family_id: u32,
    pub(super) start_key: &'a [u8],
    pub(super) end_key: &'a [u8],
    pub(super) sequence: u64,
    pub(super) provenance: SpoolProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SpoolPoint {
    pub(super) column_family_id: u32,
    pub(super) user_key: Vec<u8>,
    pub(super) sequence: u64,
    pub(super) value_type: u8,
    pub(super) value: Vec<u8>,
    pub(super) provenance: SpoolProvenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SpoolRange {
    pub(super) column_family_id: u32,
    pub(super) start_key: Vec<u8>,
    pub(super) end_key: Vec<u8>,
    pub(super) sequence: u64,
    pub(super) provenance: SpoolProvenance,
}

pub(super) struct RocksdbRecoverySpool {
    connection: Connection,
    _directory: tempfile::TempDir,
    path: PathBuf,
    point_count: u64,
    range_count: u64,
    merge_count: u64,
    range_bytes: u64,
    raw_bytes: u64,
    sealed: bool,
}

impl RocksdbRecoverySpool {
    pub(super) fn create(
        case_root: &Path,
        data_source_id: &domain::DataSourceId,
    ) -> Result<Self, CommandError> {
        let root = crate::source_db::source_staging_dir(case_root, data_source_id)
            .map_err(CommandError::from_service_error)?;
        std::fs::create_dir_all(&root).map_err(|error| CommandError::io(error.to_string()))?;
        let directory = tempfile::Builder::new()
            .prefix("ceph-rocksdb-recovery-")
            .tempdir_in(&root)
            .map_err(|error| CommandError::io(error.to_string()))?;
        let path = directory.path().join("spool.sqlite");
        let connection = Connection::open(&path).map_err(CommandError::from_service_error)?;
        configure_connection(&connection)?;
        create_schema(&connection)?;
        connection
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(CommandError::from_service_error)?;
        Ok(Self {
            connection,
            _directory: directory,
            path,
            point_count: 0,
            range_count: 0,
            merge_count: 0,
            range_bytes: 0,
            raw_bytes: 0,
            sealed: false,
        })
    }

    pub(super) fn insert_point(&mut self, input: SpoolPointInput<'_>) -> Result<(), CommandError> {
        ensure_supported_value_type(input.value_type)?;
        let added_bytes = input
            .user_key
            .len()
            .checked_add(input.value.len())
            .ok_or_else(|| spool_error("point mutation byte count overflow"))?;
        self.reserve(added_bytes, true)?;
        let result = self
            .connection
            .prepare_cached(
                "INSERT INTO point_mutations (
                column_family_id, user_key, sequence, value_type, value,
                source_kind, file_number, level, physical_offset,
                primary_ordinal, secondary_ordinal
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            )
            .and_then(|mut statement| {
                statement.execute(params![
                    input.column_family_id,
                    input.user_key,
                    input.sequence,
                    input.value_type,
                    input.value,
                    input.provenance.source_kind.encoded(),
                    input.provenance.file_number,
                    input.provenance.level,
                    input.provenance.physical_offset,
                    input.provenance.primary_ordinal,
                    input.provenance.secondary_ordinal,
                ])
            });
        match result {
            Ok(_) => {
                self.point_count += 1;
                if input.value_type == 2 {
                    self.merge_count += 1;
                }
                self.raw_bytes += added_bytes as u64;
                Ok(())
            }
            Err(error) if is_constraint_error(&error) => Err(spool_error(
                "duplicate RocksDB point internal key across the active recovery set",
            )),
            Err(error) => Err(CommandError::from_service_error(error)),
        }
    }

    pub(super) fn insert_range(&mut self, input: SpoolRangeInput<'_>) -> Result<(), CommandError> {
        if input.start_key > input.end_key {
            return Err(spool_error(
                "range tombstone start key is after its end key",
            ));
        }
        let added_bytes = input
            .start_key
            .len()
            .checked_add(input.end_key.len())
            .ok_or_else(|| spool_error("range tombstone byte count overflow"))?;
        self.reserve(added_bytes, false)?;
        let result = self
            .connection
            .prepare_cached(
                "INSERT INTO range_tombstones (
                column_family_id, start_key, end_key, sequence,
                source_kind, file_number, level, physical_offset,
                primary_ordinal, secondary_ordinal
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            )
            .and_then(|mut statement| {
                statement.execute(params![
                    input.column_family_id,
                    input.start_key,
                    input.end_key,
                    input.sequence,
                    input.provenance.source_kind.encoded(),
                    input.provenance.file_number,
                    input.provenance.level,
                    input.provenance.physical_offset,
                    input.provenance.primary_ordinal,
                    input.provenance.secondary_ordinal,
                ])
            });
        match result {
            Ok(_) => {
                self.range_count += 1;
                self.range_bytes += added_bytes as u64;
                self.raw_bytes += added_bytes as u64;
                Ok(())
            }
            Err(error) if is_constraint_error(&error) => Err(spool_error(
                "duplicate RocksDB range-deletion internal key across the active recovery set",
            )),
            Err(error) => Err(CommandError::from_service_error(error)),
        }
    }

    pub(super) fn seal(&mut self) -> Result<(), CommandError> {
        if self.sealed {
            return Err(spool_error("recovery spool was sealed more than once"));
        }
        self.connection
            .execute_batch("COMMIT; PRAGMA locking_mode=NORMAL;")
            .map_err(CommandError::from_service_error)?;
        self.sealed = true;
        Ok(())
    }

    pub(super) fn load_ranges(&self) -> Result<Vec<SpoolRange>, CommandError> {
        self.ensure_sealed()?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT column_family_id, start_key, end_key, sequence,
                        source_kind, file_number, level, physical_offset,
                        primary_ordinal, secondary_ordinal
                 FROM range_tombstones
                 ORDER BY column_family_id, start_key, sequence DESC",
            )
            .map_err(CommandError::from_service_error)?;
        let rows = statement
            .query_map([], map_range)
            .map_err(CommandError::from_service_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(CommandError::from_service_error)?
            .into_iter()
            .map(validate_range)
            .collect()
    }

    pub(super) fn visit_point_groups(
        &self,
        mut visit: impl FnMut(&[SpoolPoint]) -> Result<(), CommandError>,
    ) -> Result<(), CommandError> {
        self.ensure_sealed()?;
        let mut statement = self
            .connection
            .prepare(
                "SELECT column_family_id, user_key, sequence, value_type, value,
                        source_kind, file_number, level, physical_offset,
                        primary_ordinal, secondary_ordinal
                 FROM point_mutations
                 ORDER BY column_family_id, user_key, sequence DESC, value_type DESC",
            )
            .map_err(CommandError::from_service_error)?;
        let mut rows = statement
            .query([])
            .map_err(CommandError::from_service_error)?;
        let mut group = Vec::new();
        while let Some(row) = rows.next().map_err(CommandError::from_service_error)? {
            let point = validate_point(map_point(row).map_err(CommandError::from_service_error)?)?;
            let belongs_to_group = group.first().is_none_or(|first: &SpoolPoint| {
                first.column_family_id == point.column_family_id && first.user_key == point.user_key
            });
            if !belongs_to_group {
                visit(&group)?;
                group.clear();
            }
            group.push(point);
        }
        if !group.is_empty() {
            visit(&group)?;
        }
        Ok(())
    }

    pub(super) fn visit_point_rows_for_column(
        path: &Path,
        column_family_id: u32,
        mut visit: impl FnMut(SpoolPointRef<'_>) -> Result<(), CommandError>,
    ) -> Result<(), CommandError> {
        let connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(CommandError::from_service_error)?;
        connection
            .execute_batch("PRAGMA query_only=ON; PRAGMA temp_store=MEMORY;")
            .map_err(CommandError::from_service_error)?;
        let mut statement = connection
            .prepare(
                "SELECT column_family_id, user_key, sequence, value_type, value,
                        source_kind, file_number, level, physical_offset,
                        primary_ordinal, secondary_ordinal
                 FROM point_mutations
                 WHERE column_family_id = ?1
                 ORDER BY column_family_id, user_key, sequence DESC, value_type DESC",
            )
            .map_err(CommandError::from_service_error)?;
        let mut rows = statement
            .query([column_family_id])
            .map_err(CommandError::from_service_error)?;
        while let Some(row) = rows.next().map_err(CommandError::from_service_error)? {
            visit(map_point_ref(row)?)?;
        }
        Ok(())
    }

    pub(super) fn point_column_family_ids(&self) -> Result<Vec<u32>, CommandError> {
        self.ensure_sealed()?;
        let mut statement = self
            .connection
            .prepare("SELECT DISTINCT column_family_id FROM point_mutations ORDER BY 1")
            .map_err(CommandError::from_service_error)?;
        let column_family_ids = statement
            .query_map([], |row| row.get(0))
            .map_err(CommandError::from_service_error)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(CommandError::from_service_error)?;
        Ok(column_family_ids)
    }

    pub(super) fn point_count(&self) -> u64 {
        self.point_count
    }

    pub(super) fn range_count(&self) -> u64 {
        self.range_count
    }

    pub(super) fn merge_count(&self) -> u64 {
        self.merge_count
    }

    pub(super) fn raw_bytes(&self) -> u64 {
        self.raw_bytes
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    fn reserve(&self, added_bytes: usize, point: bool) -> Result<(), CommandError> {
        let next_count = if point {
            self.point_count.checked_add(1)
        } else {
            self.range_count.checked_add(1)
        }
        .ok_or_else(|| spool_error("recovery spool mutation count overflow"))?;
        let limit = if point {
            MAX_POINT_MUTATIONS
        } else {
            MAX_RANGE_TOMBSTONES
        };
        if next_count > limit {
            return Err(CommandError::unsupported(format!(
                "RocksDB recovery spool exceeds the {limit} {} limit",
                if point {
                    "point-mutation"
                } else {
                    "range-tombstone"
                }
            )));
        }
        if !point {
            let next_range_bytes = self
                .range_bytes
                .checked_add(added_bytes as u64)
                .ok_or_else(|| spool_error("recovery spool range byte count overflow"))?;
            if next_range_bytes > MAX_RESIDENT_RANGE_BYTES {
                return Err(CommandError::unsupported(format!(
                    "RocksDB recovery spool exceeds the {MAX_RESIDENT_RANGE_BYTES} resident range-byte limit"
                )));
            }
        }
        let next_bytes = self
            .raw_bytes
            .checked_add(added_bytes as u64)
            .ok_or_else(|| spool_error("recovery spool raw byte count overflow"))?;
        if next_bytes > MAX_RAW_BYTES {
            return Err(CommandError::unsupported(format!(
                "RocksDB recovery spool exceeds the {MAX_RAW_BYTES} raw-byte limit"
            )));
        }
        Ok(())
    }

    fn ensure_sealed(&self) -> Result<(), CommandError> {
        if self.sealed {
            Ok(())
        } else {
            Err(spool_error("recovery spool must be sealed before reading"))
        }
    }
}

impl Drop for RocksdbRecoverySpool {
    fn drop(&mut self) {
        if !self.sealed {
            let _ = self.connection.execute_batch("ROLLBACK");
        }
    }
}

fn ensure_supported_value_type(value_type: u8) -> Result<(), CommandError> {
    if matches!(value_type, 0 | 1 | 2 | 7) {
        Ok(())
    } else {
        Err(CommandError::unsupported(format!(
            "RocksDB recovery does not support value type {value_type:#04x}"
        )))
    }
}

fn is_constraint_error(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn spool_error(message: impl Into<String>) -> CommandError {
    CommandError::parser(format!("RocksDB recovery spool failed: {}", message.into()))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_rocksdb_spool.rs"]
mod tests;
