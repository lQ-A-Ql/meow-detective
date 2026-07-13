use std::collections::{BTreeMap, BTreeSet};
use std::io::SeekFrom;

use ceph_wire::{
    decode_bluefs_transaction, inspect_bluefs_transaction, BluefsExtent, BluefsFnode,
    BluefsOperation, BluefsSuper,
};
use transport::CommandError;

const BLUEFS_MAX_REPLAY_BYTES: u64 = 64 * 1024 * 1024;
const BLUEFS_MAX_REPLAY_BLOCK_SIZE: u32 = 1024 * 1024;
const BLUEFS_MAX_REPLAY_TRANSACTIONS: usize = 1_000_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BluefsReplaySnapshot {
    pub transaction_count: u32,
    pub first_sequence: u64,
    pub final_sequence: u64,
    pub logical_bytes: u64,
    pub stop_reason: String,
    pub directories: Vec<String>,
    pub files: Vec<BluefsReplayFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BluefsReplayFile {
    pub path: String,
    pub inode: u64,
    pub fnode: BluefsFnode,
}

pub(crate) fn replay_bluefs_log(
    reader: &mut dyn evidence_core::EvidenceReader,
    superblock: &BluefsSuper,
) -> Result<BluefsReplaySnapshot, CommandError> {
    validate_replay_superblock(superblock)?;
    let mut state = ReplayState::new(superblock.log_fnode.clone());
    let mut logical_offset = 0u64;
    let mut first_sequence = None;
    let mut transaction_count = 0usize;
    let stop_reason;

    loop {
        if transaction_count >= BLUEFS_MAX_REPLAY_TRANSACTIONS {
            return Err(replay_error("BlueFS replay transaction limit exceeded"));
        }
        if logical_offset >= BLUEFS_MAX_REPLAY_BYTES {
            return Err(replay_error("BlueFS replay byte limit exceeded"));
        }
        let first_block = match read_logical_range(
            reader,
            &state.log_fnode,
            logical_offset,
            u64::from(superblock.block_size),
        )? {
            Some(bytes) => bytes,
            None if transaction_count > 0 => {
                stop_reason = "extentEnd".to_string();
                break;
            }
            None => return Err(replay_error("BlueFS log has no readable first transaction")),
        };
        let prefix = match inspect_bluefs_transaction(&first_block) {
            Ok(prefix) => prefix,
            Err(_) if transaction_count > 0 => {
                stop_reason = "invalidTail".to_string();
                break;
            }
            Err(error) => return Err(replay_error(error)),
        };
        if prefix.uuid != superblock.uuid {
            if transaction_count > 0 {
                stop_reason = "uuidMismatchTail".to_string();
                break;
            }
            return Err(replay_error(
                "BlueFS first transaction UUID does not match the superblock",
            ));
        }
        if let Err(error) = state.validate_sequence(prefix.sequence) {
            if transaction_count > 0 {
                stop_reason = "sequenceMismatchTail".to_string();
                break;
            }
            return Err(error);
        }
        let encoded_length = u64::try_from(prefix.encoded_length)
            .map_err(|_| replay_error("BlueFS transaction length does not fit u64"))?;
        let aligned_length = align_up(encoded_length, u64::from(superblock.block_size))?;
        if logical_offset
            .checked_add(aligned_length)
            .is_none_or(|end| end > BLUEFS_MAX_REPLAY_BYTES)
        {
            return Err(replay_error("BlueFS transaction exceeds replay byte limit"));
        }
        let encoded = read_logical_range(reader, &state.log_fnode, logical_offset, aligned_length)?
            .ok_or_else(|| replay_error("BlueFS transaction is truncated across log extents"))?;
        if encoded.len() < prefix.encoded_length {
            if transaction_count > 0 {
                stop_reason = "truncatedTail".to_string();
                break;
            }
            return Err(replay_error(
                "BlueFS transaction is truncated across log extents",
            ));
        }
        let Some(transaction) =
            decode_replay_transaction(&encoded, prefix.encoded_length, transaction_count)?
        else {
            stop_reason = "invalidTail".to_string();
            break;
        };
        first_sequence.get_or_insert(transaction.sequence);
        let outcome = state.apply_transaction(&transaction.operations, transaction.sequence)?;
        transaction_count += 1;
        state.final_sequence = outcome.completion_sequence;

        let next_offset = outcome
            .jump_offset
            .unwrap_or_else(|| logical_offset + aligned_length);
        validate_next_offset(logical_offset, next_offset, superblock.block_size)?;
        logical_offset = next_offset;
    }

    state.finish(
        transaction_count,
        first_sequence.ok_or_else(|| replay_error("BlueFS replay produced no transactions"))?,
        logical_offset,
        stop_reason,
    )
}

fn decode_replay_transaction(
    encoded: &[u8],
    encoded_length: usize,
    transaction_count: usize,
) -> Result<Option<ceph_wire::BluefsTransaction>, CommandError> {
    match decode_bluefs_transaction(&encoded[..encoded_length]) {
        Ok(transaction) => Ok(Some(transaction)),
        Err(_) if transaction_count > 0 => Ok(None),
        Err(error) => Err(replay_error(error)),
    }
}

fn validate_next_offset(current: u64, next: u64, block_size: u32) -> Result<(), CommandError> {
    if next <= current
        || !next.is_multiple_of(u64::from(block_size))
        || next > BLUEFS_MAX_REPLAY_BYTES
    {
        return Err(replay_error("BlueFS replay jump offset is invalid"));
    }
    Ok(())
}

fn validate_replay_superblock(superblock: &BluefsSuper) -> Result<(), CommandError> {
    if superblock.block_size == 0 || !superblock.block_size.is_power_of_two() {
        return Err(replay_error(
            "BlueFS replay requires a power-of-two block size",
        ));
    }
    if superblock.block_size > BLUEFS_MAX_REPLAY_BLOCK_SIZE {
        return Err(replay_error("BlueFS replay block size exceeds the limit"));
    }
    if superblock.log_fnode.extents.is_empty() {
        return Err(replay_error(
            "BlueFS replay requires at least one log extent",
        ));
    }
    Ok(())
}

struct ReplayState {
    directories: BTreeMap<String, BTreeMap<String, u64>>,
    files: BTreeMap<u64, BluefsFnode>,
    log_fnode: BluefsFnode,
    final_sequence: u64,
}

struct TransactionOutcome {
    completion_sequence: u64,
    jump_offset: Option<u64>,
}

impl ReplayState {
    fn new(log_fnode: BluefsFnode) -> Self {
        Self {
            directories: BTreeMap::new(),
            files: BTreeMap::from([(1, log_fnode.clone())]),
            log_fnode,
            final_sequence: 0,
        }
    }

    fn validate_sequence(&self, sequence: u64) -> Result<(), CommandError> {
        let expected = self
            .final_sequence
            .checked_add(1)
            .ok_or_else(|| replay_error("BlueFS transaction sequence overflow"))?;
        if sequence != expected {
            return Err(replay_error(format!(
                "BlueFS transaction sequence {sequence} does not match expected {expected}"
            )));
        }
        Ok(())
    }

    fn apply_transaction(
        &mut self,
        operations: &[BluefsOperation],
        sequence: u64,
    ) -> Result<TransactionOutcome, CommandError> {
        let mut completion_sequence = sequence;
        let mut jump_sequence_floor = self.final_sequence;
        let mut jump_offset = None;
        for operation in operations {
            match operation {
                BluefsOperation::Init => {
                    if sequence != 1 {
                        return Err(replay_error("BlueFS INIT operation must be in sequence 1"));
                    }
                }
                BluefsOperation::AllocAdd { .. } | BluefsOperation::AllocRemove { .. } => {}
                BluefsOperation::DirectoryCreate { directory } => {
                    if self
                        .directories
                        .insert(directory.clone(), BTreeMap::new())
                        .is_some()
                    {
                        return Err(replay_error("BlueFS directory was created twice"));
                    }
                }
                BluefsOperation::DirectoryRemove { directory } => {
                    let entries = self.directories.get(directory).ok_or_else(|| {
                        replay_error("BlueFS directory removal target is missing")
                    })?;
                    if !entries.is_empty() {
                        return Err(replay_error("BlueFS cannot remove a non-empty directory"));
                    }
                    self.directories.remove(directory);
                }
                BluefsOperation::DirectoryLink {
                    directory,
                    file_name,
                    inode,
                } => self.link(directory, file_name, *inode)?,
                BluefsOperation::DirectoryUnlink {
                    directory,
                    file_name,
                } => self.unlink(directory, file_name)?,
                BluefsOperation::FileUpdate { fnode } => {
                    self.files.insert(fnode.ino, fnode.clone());
                    if fnode.ino == 1 {
                        self.log_fnode = fnode.clone();
                    }
                }
                BluefsOperation::FileUpdateIncremental { delta } => {
                    let fnode = self
                        .files
                        .get_mut(&delta.inode)
                        .ok_or_else(|| replay_error("BlueFS fnode delta target is missing"))?;
                    let allocated = allocated_bytes(&fnode.extents)?;
                    if delta.offset != allocated && !delta.extents.is_empty() {
                        return Err(replay_error(
                            "BlueFS fnode delta extents do not append at allocated end",
                        ));
                    }
                    fnode.size = delta.size;
                    fnode.mtime = delta.mtime;
                    fnode.extents.extend(delta.extents.iter().cloned());
                    fnode.encoding = delta.encoding;
                    fnode.content_size = delta.content_size;
                    if delta.inode == 1 {
                        self.log_fnode = fnode.clone();
                    }
                }
                BluefsOperation::FileRemove { inode } => {
                    if self.linked_inodes().contains(inode) {
                        return Err(replay_error("BlueFS cannot remove a linked file"));
                    }
                    self.files
                        .remove(inode)
                        .ok_or_else(|| replay_error("BlueFS file removal target is missing"))?;
                }
                BluefsOperation::Jump {
                    next_sequence,
                    offset,
                } => {
                    completion_sequence =
                        validate_jump_sequence(*next_sequence, &mut jump_sequence_floor)?;
                    jump_offset = Some(*offset);
                }
                BluefsOperation::JumpSequence { next_sequence } => {
                    completion_sequence =
                        validate_jump_sequence(*next_sequence, &mut jump_sequence_floor)?;
                }
            }
        }
        Ok(TransactionOutcome {
            completion_sequence,
            jump_offset,
        })
    }

    fn link(&mut self, directory: &str, file_name: &str, inode: u64) -> Result<(), CommandError> {
        if inode == 0 || !self.files.contains_key(&inode) {
            return Err(replay_error(
                "BlueFS directory link references a missing inode",
            ));
        }
        let entries = self
            .directories
            .get_mut(directory)
            .ok_or_else(|| replay_error("BlueFS directory link parent is missing"))?;
        if entries.insert(file_name.to_string(), inode).is_some() {
            return Err(replay_error("BlueFS directory link already exists"));
        }
        Ok(())
    }

    fn unlink(&mut self, directory: &str, file_name: &str) -> Result<(), CommandError> {
        let entries = self
            .directories
            .get_mut(directory)
            .ok_or_else(|| replay_error("BlueFS unlink parent directory is missing"))?;
        entries
            .remove(file_name)
            .ok_or_else(|| replay_error("BlueFS unlink target is missing"))?;
        Ok(())
    }

    fn linked_inodes(&self) -> BTreeSet<u64> {
        self.directories
            .values()
            .flat_map(|entries| entries.values().copied())
            .collect()
    }

    fn finish(
        self,
        transaction_count: usize,
        first_sequence: u64,
        logical_bytes: u64,
        stop_reason: String,
    ) -> Result<BluefsReplaySnapshot, CommandError> {
        let linked_inodes = self.linked_inodes();
        for inode in self.files.keys().copied().filter(|inode| *inode > 1) {
            if !linked_inodes.contains(&inode) {
                return Err(replay_error("BlueFS replay left an unlinked visible file"));
            }
        }
        let mut files = Vec::new();
        for (directory, entries) in &self.directories {
            for (file_name, inode) in entries {
                let fnode = self
                    .files
                    .get(inode)
                    .ok_or_else(|| replay_error("BlueFS snapshot link target is missing"))?;
                files.push(BluefsReplayFile {
                    path: format!("{directory}/{file_name}"),
                    inode: *inode,
                    fnode: fnode.clone(),
                });
            }
        }
        files.sort_by(|left, right| {
            left.path
                .cmp(&right.path)
                .then(left.inode.cmp(&right.inode))
        });
        Ok(BluefsReplaySnapshot {
            transaction_count: transaction_count as u32,
            first_sequence,
            final_sequence: self.final_sequence,
            logical_bytes,
            stop_reason,
            directories: self.directories.into_keys().collect(),
            files,
        })
    }
}

fn validate_jump_sequence(
    next_sequence: u64,
    jump_sequence_floor: &mut u64,
) -> Result<u64, CommandError> {
    if next_sequence <= *jump_sequence_floor {
        return Err(replay_error("BlueFS jump sequence does not move forward"));
    }
    *jump_sequence_floor = next_sequence - 1;
    Ok(next_sequence)
}

fn read_logical_range(
    reader: &mut dyn evidence_core::EvidenceReader,
    fnode: &BluefsFnode,
    logical_offset: u64,
    length: u64,
) -> Result<Option<Vec<u8>>, CommandError> {
    let logical_end = logical_offset
        .checked_add(length)
        .ok_or_else(|| replay_error("BlueFS logical read end overflow"))?;
    let allocated = allocated_bytes(&fnode.extents)?;
    if logical_offset >= allocated {
        return Ok(None);
    }
    let requested_end = logical_end.min(allocated);
    let mut output = Vec::with_capacity(
        usize::try_from(requested_end - logical_offset)
            .map_err(|_| replay_error("BlueFS read length exceeds usize"))?,
    );
    let mut logical_base = 0u64;
    for extent in &fnode.extents {
        let extent_end = logical_base
            .checked_add(u64::from(extent.length))
            .ok_or_else(|| replay_error("BlueFS extent logical end overflow"))?;
        let overlap_start = logical_offset.max(logical_base);
        let overlap_end = requested_end.min(extent_end);
        if overlap_start < overlap_end {
            let within_extent = overlap_start - logical_base;
            let physical_offset = extent
                .offset
                .checked_add(within_extent)
                .ok_or_else(|| replay_error("BlueFS physical read offset overflow"))?;
            let read_length = usize::try_from(overlap_end - overlap_start)
                .map_err(|_| replay_error("BlueFS extent read length exceeds usize"))?;
            reader
                .seek(SeekFrom::Start(physical_offset))
                .map_err(CommandError::from_service_error)?;
            let start = output.len();
            output.resize(start + read_length, 0);
            reader
                .read_exact(&mut output[start..])
                .map_err(CommandError::from_service_error)?;
        }
        logical_base = extent_end;
        if logical_base >= requested_end {
            break;
        }
    }
    Ok((output.len() as u64 == requested_end - logical_offset).then_some(output))
}

fn allocated_bytes(extents: &[BluefsExtent]) -> Result<u64, CommandError> {
    extents.iter().try_fold(0u64, |total, extent| {
        total
            .checked_add(u64::from(extent.length))
            .ok_or_else(|| replay_error("BlueFS allocated length overflow"))
    })
}

fn align_up(value: u64, alignment: u64) -> Result<u64, CommandError> {
    value
        .checked_add(alignment - 1)
        .map(|adjusted| adjusted & !(alignment - 1))
        .ok_or_else(|| replay_error("BlueFS transaction alignment overflow"))
}

fn replay_error(error: impl std::fmt::Display) -> CommandError {
    CommandError::parser(format!("BlueFS metadata log replay failed: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/import_pipeline/ceph_bluefs_replay.rs"]
mod tests;
