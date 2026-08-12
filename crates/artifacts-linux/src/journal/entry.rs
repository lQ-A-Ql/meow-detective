//! ENTRY object parsing: bind DATA objects into `JournalEntry` values.
//!
//! Layouts per `journal-def.h` (all integers little-endian, offsets relative
//! to the start of the 16-byte object header's payload):
//!
//! - ENTRY payload: seqnum(8) + realtime(8) + monotonic(8) + boot_id(16) +
//!   xor_hash(8), then items. Regular files store `{ le64 object_offset,
//!   le64 hash }` per item; COMPACT files store a single le32 offset.
//! - DATA payload: hash(8) + next_hash_offset(8) + next_field_offset(8) +
//!   entry_offset(8) + entry_array_offset(8) + n_entries(8) — plus two le32
//!   tail-entry-array fields in COMPACT files — then `FIELD=value` bytes.

use std::collections::HashMap;
use std::fmt::Write as _;

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::compress::{decode, Payload};
use super::hash::JournalHash;
use super::object::{read_object_at, read_u64_at, OBJECT_DATA, OBJECT_ENTRY};

const ENTRY_FIXED_LEN: usize = 48;
const DATA_FIXED_LEN_REGULAR: usize = 48;
const DATA_FIXED_LEN_COMPACT: usize = 56;
/// systemd's `ENTRY_FIELD_COUNT_MAX` is 1024; allow slack for odd writers.
const MAX_ENTRY_ITEMS: usize = 4096;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JournalEntry {
    pub timestamp: Option<DateTime<Utc>>,
    pub message: Option<String>,
    pub pid: Option<u32>,
    pub uid: Option<u32>,
    pub executable: Option<String>,
    pub priority: Option<u8>,
    pub boot_id: Option<String>,
    pub systemd_unit: Option<String>,
    pub hostname: Option<String>,
    pub cmdline: Option<String>,
    pub selinux_context: Option<String>,
    pub syslog_identifier: Option<String>,
    pub message_id: Option<String>,
    pub raw_fields: HashMap<String, String>,
    /// Sequence number from the ENTRY object header (monotonic per seqnum_id).
    pub seqnum: Option<u64>,
    /// Monotonic timestamp (usec) from the ENTRY object header, valid
    /// relative to `boot_id`.
    pub monotonic: Option<u64>,
}

#[derive(Debug, Default)]
pub(super) struct Counters {
    pub skipped_compressed: u64,
    pub skipped_corrupt: u64,
    pub hash_mismatches: u64,
}

pub(super) struct EntryContext<'a> {
    pub data: &'a [u8],
    /// Offset of the first object (right after the file header).
    pub header_size: u64,
    pub arena_end: u64,
    pub compact: bool,
    pub hasher: JournalHash,
}

/// Parse the ENTRY object at `offset`. Returns `None` when the object is not
/// a structurally valid ENTRY; individual broken DATA references are counted
/// in `counters` and skipped instead.
pub(super) fn parse_entry(
    ctx: &EntryContext<'_>,
    offset: u64,
    counters: &mut Counters,
) -> Option<JournalEntry> {
    let (header, payload) = read_object_at(ctx.data, offset, ctx.arena_end)?;
    if header.object_type != OBJECT_ENTRY || payload.len() < ENTRY_FIXED_LEN {
        return None;
    }

    let seqnum = read_u64_at(payload, 0)?;
    let realtime = read_u64_at(payload, 8)?;
    let monotonic = read_u64_at(payload, 16)?;
    let mut boot_id = [0u8; 16];
    boot_id.copy_from_slice(&payload[24..40]);

    let mut entry = JournalEntry {
        timestamp: micros_to_utc(realtime),
        seqnum: (seqnum != 0).then_some(seqnum),
        monotonic: (monotonic != 0).then_some(monotonic),
        ..JournalEntry::default()
    };
    if boot_id.iter().any(|byte| *byte != 0) {
        entry.boot_id = Some(id128_hex(&boot_id));
    }

    let mut fallback_ts = None;
    let items = &payload[ENTRY_FIXED_LEN..];
    if ctx.compact {
        for chunk in items.chunks_exact(4).take(MAX_ENTRY_ITEMS) {
            let object_offset = u32::from_le_bytes(chunk.try_into().unwrap_or([0; 4]));
            if object_offset != 0 {
                apply_data(
                    ctx,
                    u64::from(object_offset),
                    None,
                    &mut entry,
                    &mut fallback_ts,
                    counters,
                );
            }
        }
    } else {
        for chunk in items.chunks_exact(16).take(MAX_ENTRY_ITEMS) {
            let object_offset = read_u64_at(chunk, 0).unwrap_or(0);
            let item_hash = read_u64_at(chunk, 8).unwrap_or(0);
            if object_offset != 0 {
                apply_data(
                    ctx,
                    object_offset,
                    Some(item_hash),
                    &mut entry,
                    &mut fallback_ts,
                    counters,
                );
            }
        }
    }

    if entry.timestamp.is_none() {
        entry.timestamp = fallback_ts;
    }
    if entry.timestamp.is_none() && entry.raw_fields.is_empty() {
        return None;
    }
    Some(entry)
}

/// Read the DATA object at `offset`, decode its payload, verify hashes and
/// fold the field into `entry`. All failures are counted, never fatal.
fn apply_data(
    ctx: &EntryContext<'_>,
    offset: u64,
    item_hash: Option<u64>,
    entry: &mut JournalEntry,
    fallback_ts: &mut Option<DateTime<Utc>>,
    counters: &mut Counters,
) {
    let fixed_len = if ctx.compact {
        DATA_FIXED_LEN_COMPACT
    } else {
        DATA_FIXED_LEN_REGULAR
    };
    let Some((header, payload)) = read_object_at(ctx.data, offset, ctx.arena_end) else {
        counters.skipped_corrupt += 1;
        return;
    };
    if header.object_type != OBJECT_DATA || payload.len() < fixed_len {
        counters.skipped_corrupt += 1;
        return;
    }

    let decoded = match decode(header.flags, &payload[fixed_len..]) {
        Payload::Decoded(value) => value,
        Payload::XzUnsupported | Payload::Corrupt => {
            counters.skipped_compressed += 1;
            return;
        }
    };

    // The DATA hash and the ENTRY item hash both cover the plaintext
    // `FIELD=value` payload. Mismatches indicate corruption; the payload is
    // still used, per the spec's graceful-degradation rule.
    let computed = ctx.hasher.hash(&decoded);
    let stored = read_u64_at(payload, 0).unwrap_or(0);
    if stored != computed || item_hash.is_some_and(|hash| hash != computed) {
        counters.hash_mismatches += 1;
    }

    let Some(eq) = decoded.iter().position(|byte| *byte == b'=') else {
        counters.skipped_corrupt += 1;
        return;
    };
    if eq == 0 {
        counters.skipped_corrupt += 1;
        return;
    }
    let Ok(name) = std::str::from_utf8(&decoded[..eq]) else {
        counters.skipped_corrupt += 1;
        return;
    };
    apply_field(entry, name, &decoded[eq + 1..], fallback_ts);
}

fn apply_field(
    entry: &mut JournalEntry,
    name: &str,
    value: &[u8],
    fallback_ts: &mut Option<DateTime<Utc>>,
) {
    let text = String::from_utf8_lossy(value);
    let value = text.as_ref();
    match name {
        "MESSAGE" => entry.message = Some(value.to_string()),
        "PRIORITY" => entry.priority = value.parse().ok(),
        "_PID" => entry.pid = value.parse().ok(),
        "_UID" => entry.uid = value.parse().ok(),
        "_EXE" => entry.executable = Some(value.to_string()),
        "_CMDLINE" => entry.cmdline = Some(value.to_string()),
        "_SYSTEMD_UNIT" => entry.systemd_unit = Some(value.to_string()),
        "_HOSTNAME" => entry.hostname = Some(value.to_string()),
        "_BOOT_ID" => entry.boot_id = Some(value.to_string()),
        "_SELINUX_CONTEXT" => entry.selinux_context = Some(value.to_string()),
        "SYSLOG_IDENTIFIER" => entry.syslog_identifier = Some(value.to_string()),
        "MESSAGE_ID" => entry.message_id = Some(value.to_string()),
        "__REALTIME_TIMESTAMP" => {
            if let Ok(micros) = value.parse::<i64>() {
                *fallback_ts = Utc.timestamp_micros(micros).single();
            }
        }
        _ => {}
    }
    entry.raw_fields.insert(name.to_string(), value.to_string());
}

fn micros_to_utc(micros: u64) -> Option<DateTime<Utc>> {
    if micros == 0 {
        return None;
    }
    let micros = i64::try_from(micros).ok()?;
    Utc.timestamp_micros(micros).single()
}

fn id128_hex(id: &[u8; 16]) -> String {
    let mut out = String::with_capacity(32);
    for byte in id {
        let _ = write!(out, "{byte:02x}");
    }
    out
}
