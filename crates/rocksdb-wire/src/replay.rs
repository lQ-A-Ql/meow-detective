use std::collections::BTreeMap;

use crate::error::{Result, RocksDbWireError};
use crate::limits::ReplayLimits;
use crate::version_edit::{
    ColumnFamilyAction, InternalKeyMetadata, NewFile, NewFileFormat, NewFileMetadata, VersionEdit,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnFamilyState {
    pub id: u32,
    pub name: Vec<u8>,
    pub dropped: bool,
    pub comparator: Option<Vec<u8>>,
    pub log_number: Option<u64>,
    pub added_at_edit: Option<u64>,
    pub dropped_at_edit: Option<u64>,
    pub last_edit_ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveFile {
    pub column_family_id: u32,
    pub level: u32,
    pub file_number: u64,
    pub path_id: u32,
    pub file_size: u64,
    pub smallest: InternalKeyMetadata,
    pub largest: InternalKeyMetadata,
    pub smallest_sequence_number: u64,
    pub largest_sequence_number: u64,
    pub format: NewFileFormat,
    pub metadata: NewFileMetadata,
    pub edit_ordinal: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestSnapshot {
    pub logical_edit_count: u64,
    pub comparator: Option<Vec<u8>>,
    pub log_number: u64,
    pub previous_log_number: u64,
    pub next_file_number: u64,
    pub last_sequence: u64,
    pub min_log_number_to_keep: u64,
    pub max_column_family_id: u32,
    pub column_families: Vec<ColumnFamilyState>,
    pub live_files: Vec<LiveFile>,
}

#[derive(Debug)]
struct ReplayState {
    column_families: BTreeMap<u32, ColumnFamilyState>,
    live_files: BTreeMap<(u32, u32, u64), LiveFile>,
    file_locations: BTreeMap<u64, (u32, u32)>,
    last_log_number: Option<u64>,
    previous_log_number: Option<u64>,
    next_file_number: Option<u64>,
    last_sequence: Option<u64>,
    min_log_number_to_keep: u64,
    max_column_family_id: u32,
}

impl ReplayState {
    fn new() -> Self {
        let default = ColumnFamilyState {
            id: 0,
            name: b"default".to_vec(),
            dropped: false,
            comparator: None,
            log_number: None,
            added_at_edit: None,
            dropped_at_edit: None,
            last_edit_ordinal: 0,
        };
        Self {
            column_families: BTreeMap::from([(0, default)]),
            live_files: BTreeMap::new(),
            file_locations: BTreeMap::new(),
            last_log_number: None,
            previous_log_number: None,
            next_file_number: None,
            last_sequence: None,
            min_log_number_to_keep: 0,
            max_column_family_id: 0,
        }
    }
}

pub fn replay_version_edits(
    edits: &[VersionEdit],
    limits: ReplayLimits,
) -> Result<ManifestSnapshot> {
    let mut state = ReplayState::new();
    let mut expected_atomic_remaining = None;
    for (index, edit) in edits.iter().enumerate() {
        let ordinal = index as u64;
        validate_atomic_group(edit, ordinal, &mut expected_atomic_remaining)?;
        apply_edit(&mut state, edit, ordinal, limits)?;
    }
    if expected_atomic_remaining.is_some() {
        return Err(RocksDbWireError::InvalidAtomicGroup {
            ordinal: edits.len() as u64,
            reason: "manifest ended before the atomic group completed",
        });
    }
    finish_snapshot(state, edits.len() as u64)
}

fn validate_atomic_group(
    edit: &VersionEdit,
    ordinal: u64,
    expected_remaining: &mut Option<u32>,
) -> Result<()> {
    if edit.column_family_action.is_some() && edit.atomic_group_remaining.is_some() {
        return Err(RocksDbWireError::InvalidAtomicGroup {
            ordinal,
            reason: "column family manipulation cannot be in an atomic group",
        });
    }
    match (*expected_remaining, edit.atomic_group_remaining) {
        (None, None) => Ok(()),
        (Some(_), None) => Err(RocksDbWireError::InvalidAtomicGroup {
            ordinal,
            reason: "normal edit interrupted an atomic group",
        }),
        (Some(expected), Some(actual)) if expected != actual => {
            Err(RocksDbWireError::InvalidAtomicGroup {
                ordinal,
                reason: "remaining entry count did not decrease by one",
            })
        }
        (_, Some(actual)) => {
            *expected_remaining = actual.checked_sub(1);
            Ok(())
        }
    }
}

fn apply_edit(
    state: &mut ReplayState,
    edit: &VersionEdit,
    ordinal: u64,
    limits: ReplayLimits,
) -> Result<()> {
    if edit.column_family_action.is_some()
        && (!edit.deleted_files.is_empty() || !edit.new_files.is_empty())
    {
        return Err(RocksDbWireError::InvalidField {
            context: "column family manipulation",
            reason: "cannot include file mutations",
        });
    }
    apply_global_fields(state, edit, ordinal)?;
    let drop_completed = apply_column_family_action(state, edit, ordinal, limits)?;
    if drop_completed {
        return Ok(());
    }
    apply_column_family_fields(state, edit, ordinal)?;
    apply_deleted_files(state, edit, ordinal)?;
    apply_new_files(state, edit, ordinal, limits)
}

fn apply_global_fields(state: &mut ReplayState, edit: &VersionEdit, ordinal: u64) -> Result<()> {
    if let Some(value) = edit.previous_log_number {
        state.previous_log_number = Some(value);
    }
    if let Some(value) = edit.next_file_number {
        ensure_non_decreasing(state.next_file_number, value, ordinal, "next file number")?;
        state.next_file_number = Some(value);
    }
    if let Some(value) = edit.last_sequence {
        ensure_non_decreasing(state.last_sequence, value, ordinal, "last sequence")?;
        state.last_sequence = Some(value);
    }
    if let Some(value) = edit.min_log_number_to_keep {
        state.min_log_number_to_keep = state.min_log_number_to_keep.max(value);
    }
    if let Some(value) = edit.max_column_family_id {
        ensure_non_decreasing(
            Some(u64::from(state.max_column_family_id)),
            u64::from(value),
            ordinal,
            "maximum column family ID",
        )?;
        state.max_column_family_id = value;
    }
    Ok(())
}

fn apply_column_family_action(
    state: &mut ReplayState,
    edit: &VersionEdit,
    ordinal: u64,
    limits: ReplayLimits,
) -> Result<bool> {
    match &edit.column_family_action {
        None => Ok(false),
        Some(ColumnFamilyAction::Add { name }) => {
            add_column_family(state, edit.column_family_id, name, ordinal, limits)?;
            Ok(false)
        }
        Some(ColumnFamilyAction::Drop) => {
            drop_column_family(state, edit.column_family_id, ordinal)?;
            Ok(true)
        }
    }
}

fn add_column_family(
    state: &mut ReplayState,
    id: u32,
    name: &[u8],
    ordinal: u64,
    limits: ReplayLimits,
) -> Result<()> {
    if id == 0 || state.column_families.contains_key(&id) {
        return Err(RocksDbWireError::ColumnFamilyConflict {
            ordinal,
            column_family_id: id,
            reason: "ID already exists or is reserved for default",
        });
    }
    if state.column_families.len() >= limits.max_column_families {
        return Err(RocksDbWireError::ColumnFamilyLimit {
            limit: limits.max_column_families,
        });
    }
    state.column_families.insert(
        id,
        ColumnFamilyState {
            id,
            name: name.to_vec(),
            dropped: false,
            comparator: None,
            log_number: None,
            added_at_edit: Some(ordinal),
            dropped_at_edit: None,
            last_edit_ordinal: ordinal,
        },
    );
    state.max_column_family_id = state.max_column_family_id.max(id);
    Ok(())
}

fn drop_column_family(state: &mut ReplayState, id: u32, ordinal: u64) -> Result<()> {
    if id == 0 {
        return Err(RocksDbWireError::ColumnFamilyConflict {
            ordinal,
            column_family_id: id,
            reason: "default column family cannot be dropped",
        });
    }
    let column_family = active_column_family_mut(state, id, ordinal)?;
    column_family.dropped = true;
    column_family.dropped_at_edit = Some(ordinal);
    column_family.last_edit_ordinal = ordinal;

    let keys: Vec<_> = state
        .live_files
        .keys()
        .filter(|(column_family_id, _, _)| *column_family_id == id)
        .copied()
        .collect();
    for key in keys {
        if let Some(file) = state.live_files.remove(&key) {
            state.file_locations.remove(&file.file_number);
        }
    }
    Ok(())
}

fn apply_column_family_fields(
    state: &mut ReplayState,
    edit: &VersionEdit,
    ordinal: u64,
) -> Result<()> {
    {
        let column_family = active_column_family_mut(state, edit.column_family_id, ordinal)?;
        if let Some(comparator) = &edit.comparator {
            if let Some(existing) = &column_family.comparator {
                if existing != comparator {
                    return Err(RocksDbWireError::ColumnFamilyConflict {
                        ordinal,
                        column_family_id: edit.column_family_id,
                        reason: "comparator changed",
                    });
                }
            } else {
                column_family.comparator = Some(comparator.clone());
            }
        }
        if let Some(log_number) = edit.log_number {
            ensure_non_decreasing(
                column_family.log_number,
                log_number,
                ordinal,
                "column family log number",
            )?;
            column_family.log_number = Some(log_number);
        }
        column_family.last_edit_ordinal = ordinal;
    }
    if let Some(log_number) = edit.log_number {
        state.last_log_number = Some(log_number);
    }
    Ok(())
}

fn apply_deleted_files(state: &mut ReplayState, edit: &VersionEdit, ordinal: u64) -> Result<()> {
    for deleted in &edit.deleted_files {
        let key = (edit.column_family_id, deleted.level, deleted.file_number);
        if state.live_files.remove(&key).is_none() {
            return Err(RocksDbWireError::MissingLiveFile {
                ordinal,
                column_family_id: edit.column_family_id,
                level: deleted.level,
                file_number: deleted.file_number,
            });
        }
        state.file_locations.remove(&deleted.file_number);
    }
    Ok(())
}

fn apply_new_files(
    state: &mut ReplayState,
    edit: &VersionEdit,
    ordinal: u64,
    limits: ReplayLimits,
) -> Result<()> {
    for file in &edit.new_files {
        let identity = (edit.column_family_id, file.level);
        if let Some(existing) = state.file_locations.get(&file.file_number) {
            let reason = if *existing == identity {
                "file number is already live"
            } else {
                "file number is already live at another location"
            };
            return Err(RocksDbWireError::LiveFileConflict {
                ordinal,
                file_number: file.file_number,
                reason,
            });
        } else if state.live_files.len() >= limits.max_live_files {
            return Err(RocksDbWireError::LiveFileLimit {
                limit: limits.max_live_files,
            });
        }
        let key = (edit.column_family_id, file.level, file.file_number);
        state
            .live_files
            .insert(key, to_live_file(edit.column_family_id, file, ordinal));
        state.file_locations.insert(file.file_number, identity);
    }
    Ok(())
}

fn to_live_file(column_family_id: u32, file: &NewFile, edit_ordinal: u64) -> LiveFile {
    LiveFile {
        column_family_id,
        level: file.level,
        file_number: file.file_number,
        path_id: file.path_id,
        file_size: file.file_size,
        smallest: file.smallest.clone(),
        largest: file.largest.clone(),
        smallest_sequence_number: file.smallest_sequence_number,
        largest_sequence_number: file.largest_sequence_number,
        format: file.format,
        metadata: file.metadata.clone(),
        edit_ordinal,
    }
}

fn active_column_family_mut(
    state: &mut ReplayState,
    id: u32,
    ordinal: u64,
) -> Result<&mut ColumnFamilyState> {
    let column_family =
        state
            .column_families
            .get_mut(&id)
            .ok_or(RocksDbWireError::MissingColumnFamily {
                ordinal,
                column_family_id: id,
            })?;
    if column_family.dropped {
        return Err(RocksDbWireError::MissingColumnFamily {
            ordinal,
            column_family_id: id,
        });
    }
    Ok(column_family)
}

fn ensure_non_decreasing(
    previous: Option<u64>,
    current: u64,
    ordinal: u64,
    field: &'static str,
) -> Result<()> {
    if let Some(previous) = previous {
        if current < previous {
            return Err(RocksDbWireError::NonMonotonicField {
                ordinal,
                field,
                previous,
                current,
            });
        }
    }
    Ok(())
}

fn finish_snapshot(state: ReplayState, logical_edit_count: u64) -> Result<ManifestSnapshot> {
    let log_number = required(state.last_log_number, "log number")?;
    let encoded_next_file_number = required(state.next_file_number, "next file number")?;
    let next_file_number =
        encoded_next_file_number
            .checked_add(1)
            .ok_or(RocksDbWireError::LengthOverflow {
                context: "recovered next file number",
            })?;
    let last_sequence = required(state.last_sequence, "last sequence")?;
    validate_final_files(&state, next_file_number, last_sequence)?;
    let comparator = state
        .column_families
        .get(&0)
        .and_then(|column_family| column_family.comparator.clone());
    Ok(ManifestSnapshot {
        logical_edit_count,
        comparator,
        log_number,
        previous_log_number: state.previous_log_number.unwrap_or(0),
        next_file_number,
        last_sequence,
        min_log_number_to_keep: state.min_log_number_to_keep,
        max_column_family_id: state.max_column_family_id,
        column_families: state.column_families.into_values().collect(),
        live_files: state.live_files.into_values().collect(),
    })
}

fn validate_final_files(
    state: &ReplayState,
    next_file_number: u64,
    last_sequence: u64,
) -> Result<()> {
    for file in state.live_files.values() {
        if file.file_number >= next_file_number {
            return Err(RocksDbWireError::LiveFileConflict {
                ordinal: file.edit_ordinal,
                file_number: file.file_number,
                reason: "file number is not below next file number",
            });
        }
        if file.largest_sequence_number > last_sequence {
            return Err(RocksDbWireError::InvalidSequenceRange {
                smallest: file.smallest_sequence_number,
                largest: file.largest_sequence_number,
            });
        }
    }
    Ok(())
}

fn required<T>(value: Option<T>, field: &'static str) -> Result<T> {
    value.ok_or(RocksDbWireError::MissingRecoveryField { field })
}
