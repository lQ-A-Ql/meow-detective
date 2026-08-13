//! Top-level journal parse orchestration: header validation, entry-offset
//! collection (ENTRY_ARRAY chain walk with a linear-scan fallback), and
//! entry materialization with diagnostic counters.

use std::collections::HashSet;

use crate::LinuxArtifactError;

use super::entry::{parse_entry, Counters, EntryContext, JournalEntry};
use super::hash::JournalHash;
use super::header::Header;
use super::object::{
    next_object_offset, read_object_at, read_u64_at, OBJECT_ENTRY, OBJECT_ENTRY_ARRAY,
};

/// Hard cap on materialized entries; protects against pathological files.
const MAX_ENTRIES: usize = 1_000_000;
/// Cap on ENTRY_ARRAY chain length; real chains double in size, so this is
/// generous. Also guards against cyclic links together with the visited set.
const MAX_ENTRY_ARRAYS: usize = 4096;

/// Parse result with diagnostic counters for partially corrupt input.
#[derive(Debug, Default)]
pub struct JournalParseOutcome {
    pub entries: Vec<JournalEntry>,
    /// DATA payloads skipped because XZ/LZ4/Zstd decompression failed.
    pub skipped_compressed: u64,
    /// Objects/fields skipped because they failed structural validation.
    pub skipped_corrupt: u64,
    /// Payloads whose stored hash did not match the computed one. The
    /// payload is still used; this is corruption telemetry only.
    pub hash_mismatches: u64,
    /// The file ends before `header_size + arena_size` (typical for
    /// STATE_ONLINE files imaged while open for writing).
    pub truncated: bool,
    /// More than [`MAX_ENTRIES`] entries were present; output was cut off.
    pub entry_limit_hit: bool,
}

/// Parse a systemd journal file, returning only the entries.
pub fn parse_journal(data: &[u8]) -> Result<Vec<JournalEntry>, LinuxArtifactError> {
    Ok(parse_journal_full(data)?.entries)
}

/// Parse a systemd journal file, returning entries plus diagnostic counters.
pub fn parse_journal_full(data: &[u8]) -> Result<JournalParseOutcome, LinuxArtifactError> {
    let header = Header::parse(data)?;
    let (arena_end, truncated) = header.arena_end(data.len() as u64);

    let ctx = EntryContext {
        data,
        header_size: header.header_size,
        arena_end,
        compact: header.compact(),
        hasher: if header.keyed_hash() {
            JournalHash::SipHash(header.file_id)
        } else {
            JournalHash::Jenkins
        },
    };

    let mut counters = Counters::default();
    let offsets = collect_entry_offsets(&ctx, header.entry_array_offset);
    let entry_limit_hit = offsets.len() > MAX_ENTRIES;

    let mut entries = Vec::new();
    for offset in offsets.into_iter().take(MAX_ENTRIES) {
        match parse_entry(&ctx, offset, &mut counters) {
            Some(entry) => entries.push(entry),
            None => counters.skipped_corrupt += 1,
        }
    }

    Ok(JournalParseOutcome {
        entries,
        skipped_compressed: counters.skipped_compressed,
        skipped_corrupt: counters.skipped_corrupt,
        hash_mismatches: counters.hash_mismatches,
        truncated,
        entry_limit_hit,
    })
}

/// Collect ENTRY object offsets: walk the ENTRY_ARRAY chain referenced by
/// the header, falling back to a strict linear scan of the arena when the
/// chain is missing or broken.
fn collect_entry_offsets(ctx: &EntryContext<'_>, entry_array_offset: u64) -> Vec<u64> {
    if let Some(offsets) = walk_entry_array_chain(ctx, entry_array_offset) {
        return offsets;
    }
    linear_scan_entry_offsets(ctx)
}

/// Walk the `next_entry_array_offset` chain. Returns `None` on any
/// structural problem (bad object, cycle, oversized chain) so the caller can
/// fall back to the linear scan.
fn walk_entry_array_chain(ctx: &EntryContext<'_>, start: u64) -> Option<Vec<u64>> {
    let mut offsets = Vec::new();
    let mut visited = HashSet::new();
    let mut current = start;

    while current != 0 {
        if offsets.len() > MAX_ENTRIES
            || visited.len() >= MAX_ENTRY_ARRAYS
            || !visited.insert(current)
        {
            return None;
        }
        let (header, payload) = read_object_at(ctx.data, current, ctx.arena_end)?;
        if header.object_type != OBJECT_ENTRY_ARRAY || payload.len() < 8 {
            return None;
        }
        let next = read_u64_at(payload, 0)?;

        let items = &payload[8..];
        if ctx.compact {
            for chunk in items.chunks_exact(4) {
                let offset = u32::from_le_bytes(chunk.try_into().unwrap_or([0; 4]));
                if offset != 0 {
                    offsets.push(u64::from(offset));
                }
            }
        } else {
            for chunk in items.chunks_exact(8) {
                let offset = read_u64_at(chunk, 0)?;
                if offset != 0 {
                    offsets.push(offset);
                }
            }
        }
        current = next;
    }

    if visited.is_empty() {
        return None;
    }
    Some(offsets)
}

/// Strict linear scan of the arena: every object header must validate and
/// objects are visited in order; unknown types are skipped via their size
/// field, as the spec allows format extensions.
fn linear_scan_entry_offsets(ctx: &EntryContext<'_>) -> Vec<u64> {
    let mut offsets = Vec::new();
    let mut current = ctx.header_size;

    while current < ctx.arena_end {
        let Some((header, _)) = read_object_at(ctx.data, current, ctx.arena_end) else {
            break;
        };
        if header.object_type == OBJECT_ENTRY {
            offsets.push(current);
            if offsets.len() > MAX_ENTRIES {
                break;
            }
        }
        let Some(next) = next_object_offset(current, header.size) else {
            break;
        };
        if next <= current {
            break;
        }
        current = next;
    }
    offsets
}
