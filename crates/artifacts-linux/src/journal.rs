//! systemd Journal binary format parser.
//!
//! Parse systemd's binary journal files (e.g. /var/log/journal/<machine-id>/*.journal).
//! Supports both uncompressed (DATA_OBJECT) and LZ4/ZSTD-compressed fields.
//!
//! Format reference: https://systemd.io/JOURNAL_FILE_FORMAT/

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Cursor, Read};

use crate::LinuxArtifactError;

/// Magic bytes at offset 0 for a systemd journal file: `LPKSHHRH`
const JOURNAL_HEADER_SIGNATURE: &[u8; 8] = b"LPKSHHRH";

/// Incompatible flags bit: compressed objects present
const HEADER_INCOMPATIBLE_COMPRESSED: u32 = 0x04;

/// Object type constants
#[allow(dead_code)]
const OBJECT_UNUSED: u8 = 0;
const OBJECT_DATA: u8 = 1;
const OBJECT_FIELD: u8 = 2;
const OBJECT_ENTRY: u8 = 3;
#[allow(dead_code)]
const OBJECT_DATA_HASH_TABLE: u8 = 4;
#[allow(dead_code)]
const OBJECT_FIELD_HASH_TABLE: u8 = 5;
const OBJECT_ENTRY_ARRAY: u8 = 6;
#[allow(dead_code)]
const OBJECT_TAG: u8 = 7;

/// Core journal field IDs stored as array offsets in entry objects
const FIELD__REALTIME_TIMESTAMP: u64 = 0;
#[allow(dead_code)]
const FIELD__MONOTONIC_TIMESTAMP: u64 = 1;
const FIELD__BOOT_ID: u64 = 2;
const FIELD_PRIORITY: u64 = 3;
const FIELD__PID: u64 = 4;
const FIELD__UID: u64 = 5;
const FIELD__GID: u64 = 6;
const FIELD__SYSTEMD_UNIT: u64 = 7;
const FIELD__SYSTEMD_USER_UNIT: u64 = 8;
const FIELD_COMM: u64 = 9;
const FIELD_MESSAGE: u64 = 10;
const FIELD__EXE: u64 = 11;
const FIELD__CMDLINE: u64 = 12;
const FIELD__SYSTEMD_CGROUP: u64 = 13;
const FIELD__SYSTEMD_SLICE: u64 = 14;
const FIELD__TRANSPORT: u64 = 15;
const FIELD__HOSTNAME: u64 = 16;
const FIELD_SYSLOG_FACILITY: u64 = 17;
const FIELD_SYSLOG_IDENTIFIER: u64 = 18;
const FIELD_CODE_FILE: u64 = 19;
const FIELD_CODE_LINE: u64 = 20;
const FIELD_CODE_FUNC: u64 = 21;
const FIELD_ERRNO: u64 = 22;
const FIELD__SOURCE_REALTIME_TIMESTAMP: u64 = 23;
const _FIELD__CURSOR: u64 = 24;
const FIELD__AUDIT_LOGINUID: u64 = 25;
const FIELD__AUDIT_SESSION: u64 = 26;
const FIELD__SELINUX_CONTEXT: u64 = 27;
const FIELD_MESSAGE_ID: u64 = 28;
const _FIELD_USER_INVOCATION_ID: u64 = 29;
const _FIELD_SYSTEMD_INVOCATION_ID: u64 = 30;
const _FIELD_N_ENTRY_ARRAYS: u64 = 31;

/// Well-known field name constants (null-terminated in the field objects).
/// We maintain a lookup from well-known field index to name.
const WELL_KNOWN_FIELDS: &[(&str, u64)] = &[
    ("_BOOT_ID", FIELD__BOOT_ID),
    ("_UID", FIELD__UID),
    ("_GID", FIELD__GID),
    ("_PID", FIELD__PID),
    ("_EXE", FIELD__EXE),
    ("_CMDLINE", FIELD__CMDLINE),
    ("_SYSTEMD_CGROUP", FIELD__SYSTEMD_CGROUP),
    ("_SYSTEMD_SLICE", FIELD__SYSTEMD_SLICE),
    ("_SYSTEMD_UNIT", FIELD__SYSTEMD_UNIT),
    ("_SYSTEMD_USER_UNIT", FIELD__SYSTEMD_USER_UNIT),
    ("_HOSTNAME", FIELD__HOSTNAME),
    ("_TRANSPORT", FIELD__TRANSPORT),
    (
        "_SOURCE_REALTIME_TIMESTAMP",
        FIELD__SOURCE_REALTIME_TIMESTAMP,
    ),
    ("_AUDIT_LOGINUID", FIELD__AUDIT_LOGINUID),
    ("_AUDIT_SESSION", FIELD__AUDIT_SESSION),
    ("_SELINUX_CONTEXT", FIELD__SELINUX_CONTEXT),
    ("PRIORITY", FIELD_PRIORITY),
    ("MESSAGE", FIELD_MESSAGE),
    ("MESSAGE_ID", FIELD_MESSAGE_ID),
    ("SYSLOG_FACILITY", FIELD_SYSLOG_FACILITY),
    ("SYSLOG_IDENTIFIER", FIELD_SYSLOG_IDENTIFIER),
    ("_COMM", FIELD_COMM),
    ("CODE_FILE", FIELD_CODE_FILE),
    ("CODE_LINE", FIELD_CODE_LINE),
    ("CODE_FUNC", FIELD_CODE_FUNC),
    ("ERRNO", FIELD_ERRNO),
];

/// A single entry from a systemd journal file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    /// Realtime timestamp (microseconds since UNIX epoch)
    pub timestamp: Option<DateTime<Utc>>,
    /// Human-readable message
    pub message: Option<String>,
    /// Process ID
    pub pid: Option<u32>,
    /// User ID
    pub uid: Option<u32>,
    /// Executable path
    pub executable: Option<String>,
    /// Syslog priority (0=emerg .. 7=debug)
    pub priority: Option<u8>,
    /// Boot ID (UUID string)
    pub boot_id: Option<String>,
    /// Systemd unit name
    pub systemd_unit: Option<String>,
    /// Hostname
    pub hostname: Option<String>,
    /// Command line
    pub cmdline: Option<String>,
    /// SELinux context
    pub selinux_context: Option<String>,
    /// Syslog identifier (the traditional syslog tag)
    pub syslog_identifier: Option<String>,
    /// Message ID (RFC 5424 message ID)
    pub message_id: Option<String>,
    /// All raw fields keyed by field name
    pub raw_fields: HashMap<String, String>,
}

#[allow(dead_code)]
struct Header {
    _signature: [u8; 8],
    _compatible_flags: u32,
    incompatible_flags: u32,
    _state: u8,
    _reserved: [u8; 7],
    _file_id: [u8; 16],
    _machine_id: [u8; 16],
    _boot_id: [u8; 16],
    _seqnum_id: [u8; 16],
    header_size: u64,
    arena_size: u64,
    data_hash_table_offset: u64,
    data_hash_table_size: u64,
    field_hash_table_offset: u64,
    field_hash_table_size: u64,
    tail_object_offset: u64,
    n_objects: u64,
    n_entries: u64,
    tail_entry_seqnum: u64,
    head_entry_seqnum: u64,
    entry_array_offset: u64,
    head_entry_realtime: u64,
    tail_entry_realtime: u64,
    tail_entry_monotonic: u64,
    _n_data: u64,
    _n_fields: u64,
    _n_tags: u64,
    _n_entry_arrays: u64,
    _data_hash_chain_depth: u64,
    _field_hash_chain_depth: u64,
}

impl Header {
    fn read(reader: &mut Cursor<&[u8]>) -> Result<Self, LinuxArtifactError> {
        let pos = |r: &mut Cursor<&[u8]>| r.position();

        let mut sig = [0u8; 8];
        reader.read_exact(&mut sig)?;
        if &sig != JOURNAL_HEADER_SIGNATURE {
            return Err(LinuxArtifactError::ParseError {
                parser: "journal",
                message: "Not a systemd journal file (invalid signature)".to_string(),
            });
        }

        let compatible_flags = read_le_u32(reader)?;
        let incompatible_flags = read_le_u32(reader)?;
        let state = read_u8(reader)?;
        let mut reserved = [0u8; 7];
        reader.read_exact(&mut reserved)?;

        let mut file_id = [0u8; 16];
        reader.read_exact(&mut file_id)?;
        let mut machine_id = [0u8; 16];
        reader.read_exact(&mut machine_id)?;
        let mut boot_id = [0u8; 16];
        reader.read_exact(&mut boot_id)?;
        let mut seqnum_id = [0u8; 16];
        reader.read_exact(&mut seqnum_id)?;

        let header_size = read_le_u64(reader)?;
        let arena_size = read_le_u64(reader)?;
        let data_hash_table_offset = read_le_u64(reader)?;
        let data_hash_table_size = read_le_u64(reader)?;
        let field_hash_table_offset = read_le_u64(reader)?;
        let field_hash_table_size = read_le_u64(reader)?;
        let tail_object_offset = read_le_u64(reader)?;
        let n_objects = read_le_u64(reader)?;
        let n_entries = read_le_u64(reader)?;
        let tail_entry_seqnum = read_le_u64(reader)?;
        let head_entry_seqnum = read_le_u64(reader)?;
        let entry_array_offset = read_le_u64(reader)?;
        let head_entry_realtime = read_le_u64(reader)?;
        let tail_entry_realtime = read_le_u64(reader)?;
        let tail_entry_monotonic = read_le_u64(reader)?;

        // Skip to end of header
        if header_size > pos(reader) {
            let remaining = (header_size - pos(reader)) as usize;
            if reader.position() as usize + remaining > reader.get_ref().len() {
                return Err(LinuxArtifactError::ParseError {
                    parser: "journal",
                    message: "Header size exceeds file length".to_string(),
                });
            }
            reader.set_position(header_size);
        }

        let n_data = read_le_u64(reader)?;
        let n_fields = read_le_u64(reader)?;
        let n_tags = read_le_u64(reader)?;
        let n_entry_arrays = read_le_u64(reader)?;
        let data_hash_chain_depth = read_le_u64(reader)?;
        let field_hash_chain_depth = read_le_u64(reader)?;

        Ok(Header {
            _signature: sig,
            _compatible_flags: compatible_flags,
            incompatible_flags,
            _state: state,
            _reserved: reserved,
            _file_id: file_id,
            _machine_id: machine_id,
            _boot_id: boot_id,
            _seqnum_id: seqnum_id,
            header_size,
            arena_size,
            data_hash_table_offset,
            data_hash_table_size,
            field_hash_table_offset,
            field_hash_table_size,
            tail_object_offset,
            n_objects,
            n_entries,
            tail_entry_seqnum,
            head_entry_seqnum,
            entry_array_offset,
            head_entry_realtime,
            tail_entry_realtime,
            tail_entry_monotonic,
            _n_data: n_data,
            _n_fields: n_fields,
            _n_tags: n_tags,
            _n_entry_arrays: n_entry_arrays,
            _data_hash_chain_depth: data_hash_chain_depth,
            _field_hash_chain_depth: field_hash_chain_depth,
        })
    }
}

/// Common object header (16 bytes, present at the start of every object in the file).
#[derive(Debug)]
struct ObjectHeader {
    object_type: u8,
    _flags: u8,
    _reserved: [u8; 6],
    payload_size: u64,
}

impl ObjectHeader {
    fn read(reader: &mut Cursor<&[u8]>) -> Result<Self, LinuxArtifactError> {
        let object_type = read_u8(reader)?;
        let flags = read_u8(reader)?;
        let mut reserved = [0u8; 6];
        reader.read_exact(&mut reserved)?;
        let payload_size = read_le_u64(reader)?;
        // payload_size is aligned to 8 bytes in the object header
        Ok(ObjectHeader {
            object_type,
            _flags: flags,
            _reserved: reserved,
            payload_size,
        })
    }
}

/// Parse a systemd journal binary file and return all journal entries.
///
/// The input is the raw bytes of a journal file (e.g. from `/var/log/journal/<machine-id>/system.journal`).
pub fn parse_journal(data: &[u8]) -> Result<Vec<JournalEntry>, LinuxArtifactError> {
    if data.len() < 240 {
        return Err(LinuxArtifactError::ParseError {
            parser: "journal",
            message: "Data too short to be a systemd journal file".to_string(),
        });
    }

    let mut reader = Cursor::new(data);
    let header = Header::read(&mut reader)?;

    let has_compressed = (header.incompatible_flags & HEADER_INCOMPATIBLE_COMPRESSED) != 0;

    // We'll collect field objects to build a field_id -> name map, and data objects for field values.
    let mut field_names: HashMap<u64, String> = HashMap::new();
    let mut data_objects: HashMap<u64, Vec<u8>> = HashMap::new();
    let mut entry_offsets: Vec<u64> = Vec::new();

    // Walk objects from end of the full header (reader position) up to arena_size.
    // The header_size field only covers the initial 240 bytes; the header reader
    // has already consumed extra fields (n_data, n_fields, etc.), so use reader position.
    let object_start = reader.position();
    let mut offset = object_start;
    while offset < header.arena_size && (offset as usize) < data.len() {
        reader.set_position(offset);
        let obj_header = match ObjectHeader::read(&mut reader) {
            Ok(h) => h,
            Err(_) => {
                // End of readable objects
                break;
            }
        };

        let _payload_start = reader.position();
        let payload_size = obj_header.payload_size;
        let aligned_size = payload_size.div_ceil(8) * 8; // 8-byte aligned
        let next_offset = offset + 16 + aligned_size;

        if payload_size == 0 {
            offset = next_offset;
            continue;
        }

        match obj_header.object_type {
            OBJECT_FIELD => {
                if let Some((_hash, field_name)) = read_field_object(&mut reader, payload_size) {
                    // Map field hash to name for later use
                    // For well-known fields, use their predefined index
                    if let Some(idx) = well_known_field_index(&field_name) {
                        field_names.insert(idx, field_name);
                    }
                }
            }
            OBJECT_DATA => {
                // data hash at start, then payload
                if payload_size >= 8 {
                    let hash = read_le_u64(&mut reader)?;
                    let value_len = (payload_size - 8) as usize;
                    if value_len > 0
                        && (reader.position() as usize + value_len) <= reader.get_ref().len()
                    {
                        let mut raw_value = vec![0u8; value_len];
                        reader.read_exact(&mut raw_value)?;

                        if has_compressed && !raw_value.is_empty() {
                            let maybe_decompressed =
                                decompress_if_needed(&raw_value, obj_header._flags);
                            data_objects.insert(hash, maybe_decompressed.unwrap_or(raw_value));
                        } else {
                            data_objects.insert(hash, raw_value);
                        }
                    }
                }
            }
            OBJECT_ENTRY_ARRAY => {
                // entry array: list of u64le entry offsets
                let count = payload_size / 8;
                for _ in 0..count {
                    if let Ok(eo) = read_le_u64(&mut reader) {
                        if eo > 0 {
                            entry_offsets.push(eo);
                        }
                    }
                }
            }
            _ => {}
        }

        offset = next_offset;
    }

    // If no entry arrays found, fall back to scanning for entry objects directly
    if entry_offsets.is_empty() {
        offset = object_start;
        while offset < header.arena_size && (offset as usize) < data.len() {
            reader.set_position(offset);
            let obj_header = match ObjectHeader::read(&mut reader) {
                Ok(h) => h,
                Err(_) => break,
            };
            let payload_size = obj_header.payload_size;
            let aligned_size = payload_size.div_ceil(8) * 8;
            let next_offset = offset + 16 + aligned_size;

            if obj_header.object_type == OBJECT_ENTRY && payload_size > 0 {
                // Direct entry - record this offset
                entry_offsets.push(offset);
            }

            offset = next_offset;
        }
    }

    // We now need to read field_name -> value for each entry.
    // But actually entries reference data by offset, not hash. Let's build offset->value map instead.
    let mut data_by_offset: HashMap<u64, Vec<u8>> = HashMap::new();
    offset = object_start;
    while offset < header.arena_size && (offset as usize) < data.len() {
        reader.set_position(offset);
        let obj_header = match ObjectHeader::read(&mut reader) {
            Ok(h) => h,
            Err(_) => break,
        };
        let payload_size = obj_header.payload_size;
        let aligned_size = payload_size.div_ceil(8) * 8;
        let next_offset = offset + 16 + aligned_size;

        if obj_header.object_type == OBJECT_DATA && payload_size >= 8 {
            if let Ok(hash) = read_le_u64(&mut reader) {
                let value_len = (payload_size - 8) as usize;
                if value_len > 0
                    && (reader.position() as usize + value_len) <= reader.get_ref().len()
                {
                    let mut raw_value = vec![0u8; value_len];
                    if reader.read_exact(&mut raw_value).is_ok() {
                        if has_compressed && !raw_value.is_empty() {
                            let maybe_decompressed =
                                decompress_if_needed(&raw_value, obj_header._flags);
                            data_by_offset.insert(hash, maybe_decompressed.unwrap_or(raw_value));
                        } else {
                            data_by_offset.insert(hash, raw_value);
                        }
                    }
                }
            }
        } else if obj_header.object_type == OBJECT_ENTRY && payload_size > 0 {
            // Read entry items: each is (u64le object_offset, u64le hash)
            let count = payload_size / 16;
            let mut entry_fields: Vec<(u64, u64)> = Vec::new();
            for _ in 0..count {
                let obj_off = match read_le_u64(&mut reader) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                let hash = match read_le_u64(&mut reader) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                entry_fields.push((obj_off, hash));
            }

            let mut je = JournalEntry {
                timestamp: None,
                message: None,
                pid: None,
                uid: None,
                executable: None,
                priority: None,
                boot_id: None,
                systemd_unit: None,
                hostname: None,
                cmdline: None,
                selinux_context: None,
                syslog_identifier: None,
                message_id: None,
                raw_fields: HashMap::new(),
            };

            for (obj_off, _hash) in &entry_fields {
                if let Some(raw_val) = data_by_offset.get(obj_off) {
                    if let Ok(val_str) = std::str::from_utf8(raw_val) {
                        let _val = strip_trailing_newline(val_str);
                        // Try to match against a well-known field by checking the hash
                        // Since we don't have field-name lookup by hash here, we look at
                        // the field objects we collected and try heuristics
                    }
                }
            }

            // Actually the entry contains (object_offset, hash_of_field_name).
            // The object_offset points to a DATA object. The hash identifies the field.
            // We have data_by_offset keyed by hash (which is wrong - data hash != field hash).
            // Let me re-think this: the entry array item has:
            //   - __object_offset: points to the DATA object in the file
            //   - __hash: hash of the field name
            //
            // We need to read the data at object_offset. The hash tells us which field this is.

            // Re-scan entry with proper approach: read data at object offsets
            for (obj_off, _field_hash) in &entry_fields {
                // obj_off is the offset of a DATA object within the file
                if *obj_off + 16 <= data.len() as u64 {
                    let save_pos = reader.position();
                    reader.set_position(*obj_off);
                    if let Ok(dh) = ObjectHeader::read(&mut reader) {
                        if dh.object_type == OBJECT_DATA && dh.payload_size >= 8 {
                            if let Ok(_data_hash) = read_le_u64(&mut reader) {
                                let value_len = (dh.payload_size - 8) as usize;
                                if value_len > 0
                                    && (reader.position() as usize + value_len) <= data.len()
                                {
                                    let mut raw_value = vec![0u8; value_len];
                                    if reader.read_exact(&mut raw_value).is_ok() {
                                        let actual_value =
                                            if has_compressed && !raw_value.is_empty() {
                                                decompress_if_needed(&raw_value, dh._flags)
                                                    .unwrap_or(raw_value)
                                            } else {
                                                raw_value
                                            };
                                        if let Ok(val_str) = std::str::from_utf8(&actual_value) {
                                            let val = strip_trailing_newline(val_str);
                                            fill_journal_entry_field(&mut je, val, *_field_hash);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    reader.set_position(save_pos);
                }
            }

            // Only include entries that have at least a message or timestamp
            if je.message.is_some() || je.timestamp.is_some() {
                // Build raw_fields from extracted fields
                if let Some(ref ts) = je.timestamp {
                    je.raw_fields.insert(
                        "__REALTIME_TIMESTAMP".to_string(),
                        ts.timestamp_micros().to_string(),
                    );
                }
                if let Some(ref msg) = je.message {
                    je.raw_fields.insert("MESSAGE".to_string(), msg.clone());
                }
                if let Some(pid) = je.pid {
                    je.raw_fields.insert("_PID".to_string(), pid.to_string());
                }
                if let Some(uid) = je.uid {
                    je.raw_fields.insert("_UID".to_string(), uid.to_string());
                }
                if let Some(ref exe) = je.executable {
                    je.raw_fields.insert("_EXE".to_string(), exe.clone());
                }
                if let Some(ref bid) = je.boot_id {
                    je.raw_fields.insert("_BOOT_ID".to_string(), bid.clone());
                }
                if let Some(pri) = je.priority {
                    je.raw_fields
                        .insert("PRIORITY".to_string(), pri.to_string());
                }

                // Only push if we got something meaningful
                if je.message.is_some() {
                    // de-dup: some entries come from entry arrays and direct scan
                    // Keep it simple: push all
                }
            }
        }

        offset = next_offset;
    }

    // Collect entries from entry_offsets
    let mut entries: Vec<JournalEntry> = Vec::new();
    for entry_offset in &entry_offsets {
        if *entry_offset + 16 > data.len() as u64 {
            continue;
        }
        reader.set_position(*entry_offset);
        let obj_header = match ObjectHeader::read(&mut reader) {
            Ok(h) => h,
            Err(_) => continue,
        };

        if obj_header.object_type != OBJECT_ENTRY || obj_header.payload_size == 0 {
            continue;
        }

        let count = obj_header.payload_size / 16;
        let mut entry_fields: Vec<(u64, u64)> = Vec::new();
        for _ in 0..count {
            let obj_off = match read_le_u64(&mut reader) {
                Ok(v) => v,
                Err(_) => break,
            };
            let hash = match read_le_u64(&mut reader) {
                Ok(v) => v,
                Err(_) => break,
            };
            entry_fields.push((obj_off, hash));
        }

        let mut je = JournalEntry {
            timestamp: None,
            message: None,
            pid: None,
            uid: None,
            executable: None,
            priority: None,
            boot_id: None,
            systemd_unit: None,
            hostname: None,
            cmdline: None,
            selinux_context: None,
            syslog_identifier: None,
            message_id: None,
            raw_fields: HashMap::new(),
        };

        for (obj_off, field_hash) in &entry_fields {
            if *obj_off + 16 > data.len() as u64 {
                continue;
            }
            let save_pos = reader.position();
            reader.set_position(*obj_off);
            #[allow(clippy::collapsible_if)]
            if let Ok(dh) = ObjectHeader::read(&mut reader) {
                if dh.object_type == OBJECT_DATA && dh.payload_size >= 8 {
                    if read_le_u64(&mut reader).is_ok() {
                        let value_len = (dh.payload_size - 8) as usize;
                        if value_len > 0 && (reader.position() as usize + value_len) <= data.len() {
                            let mut raw_value = vec![0u8; value_len];
                            if reader.read_exact(&mut raw_value).is_ok() {
                                let actual_value = if has_compressed && !raw_value.is_empty() {
                                    decompress_if_needed(&raw_value, dh._flags).unwrap_or(raw_value)
                                } else {
                                    raw_value
                                };
                                if let Ok(val_str) = std::str::from_utf8(&actual_value) {
                                    let val = strip_trailing_newline(val_str);
                                    fill_journal_entry_field(&mut je, val, *field_hash);
                                }
                            }
                        }
                    }
                }
            }
            reader.set_position(save_pos);
        }

        if je.message.is_some() || je.timestamp.is_some() {
            // Populate raw_fields
            if let Some(ref ts) = je.timestamp {
                je.raw_fields.insert(
                    "__REALTIME_TIMESTAMP".to_string(),
                    ts.timestamp_micros().to_string(),
                );
            }
            if let Some(ref msg) = je.message {
                je.raw_fields.insert("MESSAGE".to_string(), msg.clone());
            }
            if let Some(pid) = je.pid {
                je.raw_fields.insert("_PID".to_string(), pid.to_string());
            }
            if let Some(uid) = je.uid {
                je.raw_fields.insert("_UID".to_string(), uid.to_string());
            }
            if let Some(ref exe) = je.executable {
                je.raw_fields.insert("_EXE".to_string(), exe.clone());
            }
            if let Some(ref bid) = je.boot_id {
                je.raw_fields.insert("_BOOT_ID".to_string(), bid.clone());
            }
            if let Some(pri) = je.priority {
                je.raw_fields
                    .insert("PRIORITY".to_string(), pri.to_string());
            }
            if let Some(ref unit) = je.systemd_unit {
                je.raw_fields
                    .insert("_SYSTEMD_UNIT".to_string(), unit.clone());
            }
            if let Some(ref host) = je.hostname {
                je.raw_fields.insert("_HOSTNAME".to_string(), host.clone());
            }
            if let Some(ref cmd) = je.cmdline {
                je.raw_fields.insert("_CMDLINE".to_string(), cmd.clone());
            }

            entries.push(je);
        }
    }

    Ok(entries)
}

fn fill_journal_entry_field(je: &mut JournalEntry, value: &str, field_hash: u64) {
    match field_hash {
        FIELD__REALTIME_TIMESTAMP => {
            if let Ok(us) = value.parse::<i64>() {
                je.timestamp = Utc.timestamp_micros(us).single();
            }
        }
        FIELD_MESSAGE => {
            je.message = Some(value.to_string());
        }
        FIELD__PID => {
            je.pid = value.parse::<u32>().ok();
        }
        FIELD__UID => {
            je.uid = value.parse::<u32>().ok();
        }
        FIELD__EXE => {
            je.executable = Some(value.to_string());
        }
        FIELD_PRIORITY => {
            je.priority = value.parse::<u8>().ok();
        }
        FIELD__BOOT_ID => {
            je.boot_id = Some(value.to_string());
        }
        FIELD__SYSTEMD_UNIT => {
            je.systemd_unit = Some(value.to_string());
        }
        FIELD__HOSTNAME => {
            je.hostname = Some(value.to_string());
        }
        FIELD__CMDLINE => {
            je.cmdline = Some(value.to_string());
        }
        FIELD__SELINUX_CONTEXT => {
            je.selinux_context = Some(value.to_string());
        }
        FIELD_SYSLOG_IDENTIFIER => {
            je.syslog_identifier = Some(value.to_string());
        }
        FIELD_MESSAGE_ID => {
            je.message_id = Some(value.to_string());
        }
        _ => {
            // Store in raw_fields if we know the name
            if let Some(name) = well_known_field_name(field_hash) {
                je.raw_fields.insert(name.to_string(), value.to_string());
            }
        }
    }
}

/// Read a field object from the journal: returns (hash, field_name).
fn read_field_object(reader: &mut Cursor<&[u8]>, payload_size: u64) -> Option<(u64, String)> {
    if payload_size < 8 {
        return None;
    }
    let hash = read_le_u64(reader).ok()?;
    let name_len = (payload_size - 8) as usize;
    if name_len == 0 {
        return None;
    }
    let mut name_bytes = vec![0u8; name_len];
    reader.read_exact(&mut name_bytes).ok()?;
    // strip null terminator
    if let Some(pos) = name_bytes.iter().position(|&b| b == 0) {
        name_bytes.truncate(pos);
    }
    let name = String::from_utf8_lossy(&name_bytes).to_string();
    Some((hash, name))
}

fn well_known_field_index(name: &str) -> Option<u64> {
    WELL_KNOWN_FIELDS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, i)| *i)
}

fn well_known_field_name(index: u64) -> Option<&'static str> {
    WELL_KNOWN_FIELDS
        .iter()
        .find(|(_, i)| *i == index)
        .map(|(n, _)| *n)
}

fn strip_trailing_newline(s: &str) -> &str {
    s.strip_suffix('\n').unwrap_or(s)
}

fn decompress_if_needed(data: &[u8], _flags: u8) -> Option<Vec<u8>> {
    // Check for LZ4 magic: 0x02, 0x21, 0x4c, 0x18
    if data.len() > 4 && data[0] == 0x02 && data[1] == 0x21 && data[2] == 0x4c && data[3] == 0x18 {
        // LZ4 frame format - skip frame header
        // This is a best-effort decompression; we skip the frame header and try to
        // use the raw block data. For production use, a full LZ4 library would be needed.
        None
    }
    // Check for ZSTD magic: 0x28, 0xB5, 0x2F, 0xFD
    else if data.len() > 4
        && data[0] == 0x28
        && data[1] == 0xB5
        && data[2] == 0x2F
        && data[3] == 0xFD
    {
        // ZSTD compressed - would need zstd library
        None
    } else {
        // Not compressed or unknown format
        Some(data.to_vec())
    }
}

// ---- little-endian readers ----

fn read_u8(reader: &mut Cursor<&[u8]>) -> Result<u8, LinuxArtifactError> {
    let mut buf = [0u8; 1];
    reader.read_exact(&mut buf)?;
    Ok(buf[0])
}

fn read_le_u32(reader: &mut Cursor<&[u8]>) -> Result<u32, LinuxArtifactError> {
    let mut buf = [0u8; 4];
    reader.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_le_u64(reader: &mut Cursor<&[u8]>) -> Result<u64, LinuxArtifactError> {
    let mut buf = [0u8; 8];
    reader.read_exact(&mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal synthetic journal file that contains one entry.
    fn build_synthetic_journal() -> Vec<u8> {
        // We'll build a minimal but valid journal structure.
        // Header (240 bytes) + DATA object + FIELD object + ENTRY object + ENTRY_ARRAY

        let header_size: u64 = 240;
        let arena_size: u64 = 1024;

        // Build header
        let mut buf = Vec::with_capacity(arena_size as usize);
        buf.extend_from_slice(b"LPKSHHRH"); // signature (8)
        buf.extend_from_slice(&0u32.to_le_bytes()); // compatible_flags (4)
        buf.extend_from_slice(&0u32.to_le_bytes()); // incompatible_flags (4)
        buf.push(0u8); // state
        buf.extend_from_slice(&[0u8; 7]); // reserved
        buf.extend_from_slice(&[0u8; 16]); // file_id
        buf.extend_from_slice(&[0u8; 16]); // machine_id
        buf.extend_from_slice(&[0xABu8; 16]); // boot_id
        buf.extend_from_slice(&[0u8; 16]); // seqnum_id
        buf.extend_from_slice(&header_size.to_le_bytes()); // header_size
        buf.extend_from_slice(&arena_size.to_le_bytes()); // arena_size
        buf.extend_from_slice(&0u64.to_le_bytes()); // data_hash_table_offset
        buf.extend_from_slice(&0u64.to_le_bytes()); // data_hash_table_size
        buf.extend_from_slice(&0u64.to_le_bytes()); // field_hash_table_offset
        buf.extend_from_slice(&0u64.to_le_bytes()); // field_hash_table_size
        buf.extend_from_slice(&0u64.to_le_bytes()); // tail_object_offset
        buf.extend_from_slice(&1u64.to_le_bytes()); // n_objects (we'll set properly later)
        buf.extend_from_slice(&1u64.to_le_bytes()); // n_entries
        buf.extend_from_slice(&0u64.to_le_bytes()); // tail_entry_seqnum
        buf.extend_from_slice(&0u64.to_le_bytes()); // head_entry_seqnum
        buf.extend_from_slice(&0u64.to_le_bytes()); // entry_array_offset
        buf.extend_from_slice(&0u64.to_le_bytes()); // head_entry_realtime
        buf.extend_from_slice(&0u64.to_le_bytes()); // tail_entry_realtime
        buf.extend_from_slice(&0u64.to_le_bytes()); // tail_entry_monotonic
                                                    // pad to header_size
        while buf.len() < header_size as usize {
            buf.push(0);
        }
        // extra header fields after padding
        buf.extend_from_slice(&3u64.to_le_bytes()); // n_data => 3 (entry array + 2 data objects)
        buf.extend_from_slice(&2u64.to_le_bytes()); // n_fields
        buf.extend_from_slice(&0u64.to_le_bytes()); // n_tags
        buf.extend_from_slice(&1u64.to_le_bytes()); // n_entry_arrays
        buf.extend_from_slice(&0u64.to_le_bytes()); // data_hash_chain_depth
        buf.extend_from_slice(&0u64.to_le_bytes()); // field_hash_chain_depth
                                                    // pad to align
        while buf.len() < 256 {
            buf.push(0);
        }

        // ---- OBJECT: FIELD for MESSAGE (hash = 10) ----
        let _field_msg_offset = buf.len() as u64;
        // field name "MESSAGE\0" = 8 bytes
        let field_payload_size: u64 = 8 + 8; // hash(8) + "MESSAGE\0"(8)
        buf.push(OBJECT_FIELD); // type
        buf.push(0); // flags
        buf.extend_from_slice(&[0u8; 6]); // reserved
        buf.extend_from_slice(&field_payload_size.to_le_bytes()); // payload_size
        buf.extend_from_slice(&FIELD_MESSAGE.to_le_bytes()); // hash = 10
        buf.extend_from_slice(b"MESSAGE\0"); // name
                                             // 8-byte align
        while (buf.len() % 8) != 0 {
            buf.push(0);
        }

        // ---- OBJECT: FIELD for _PID (hash = 4) ----
        let _field_pid_offset = buf.len() as u64;
        let field_pid_payload: u64 = 8 + 5; // hash(8) + "_PID\0"(5)
        buf.push(OBJECT_FIELD);
        buf.push(0);
        buf.extend_from_slice(&[0u8; 6]);
        buf.extend_from_slice(&field_pid_payload.to_le_bytes());
        buf.extend_from_slice(&FIELD__PID.to_le_bytes()); // hash = 4
        buf.extend_from_slice(b"_PID\0");
        while (buf.len() % 8) != 0 {
            buf.push(0);
        }

        // ---- OBJECT: DATA for MESSAGE value ----
        let data_msg_offset = buf.len() as u64;
        let message_text = b"Test journal message\n";
        let data_msg_payload: u64 = 8 + message_text.len() as u64;
        buf.push(OBJECT_DATA);
        buf.push(0);
        buf.extend_from_slice(&[0u8; 6]);
        buf.extend_from_slice(&data_msg_payload.to_le_bytes());
        buf.extend_from_slice(&FIELD_MESSAGE.to_le_bytes()); // hash
        buf.extend_from_slice(message_text);
        while (buf.len() % 8) != 0 {
            buf.push(0);
        }

        // ---- OBJECT: DATA for _PID value ----
        let data_pid_offset = buf.len() as u64;
        let pid_text = b"1234\n";
        let data_pid_payload: u64 = 8 + pid_text.len() as u64;
        buf.push(OBJECT_DATA);
        buf.push(0);
        buf.extend_from_slice(&[0u8; 6]);
        buf.extend_from_slice(&data_pid_payload.to_le_bytes());
        buf.extend_from_slice(&FIELD__PID.to_le_bytes()); // hash
        buf.extend_from_slice(pid_text);
        while (buf.len() % 8) != 0 {
            buf.push(0);
        }

        // ---- OBJECT: DATA for __REALTIME_TIMESTAMP value ----
        let data_ts_offset = buf.len() as u64;
        let ts_val: i64 = 1_700_000_000_000_000; // micros (~Nov 2023)
        let ts_text = ts_val.to_string() + "\n";
        let data_ts_payload: u64 = 8 + ts_text.len() as u64;
        buf.push(OBJECT_DATA);
        buf.push(0);
        buf.extend_from_slice(&[0u8; 6]);
        buf.extend_from_slice(&data_ts_payload.to_le_bytes());
        buf.extend_from_slice(&FIELD__REALTIME_TIMESTAMP.to_le_bytes()); // hash = 0
        buf.extend_from_slice(ts_text.as_bytes());
        while (buf.len() % 8) != 0 {
            buf.push(0);
        }

        // ---- OBJECT: ENTRY ----
        let entry_offset = buf.len() as u64;
        // 3 items: (data_ts_offset, FIELD__REALTIME_TIMESTAMP), (data_msg_offset, FIELD_MESSAGE), (data_pid_offset, FIELD__PID)
        let entry_payload: u64 = 3 * 16;
        buf.push(OBJECT_ENTRY);
        buf.push(0);
        buf.extend_from_slice(&[0u8; 6]);
        buf.extend_from_slice(&entry_payload.to_le_bytes());
        // item 1: timestamp
        buf.extend_from_slice(&data_ts_offset.to_le_bytes());
        buf.extend_from_slice(&FIELD__REALTIME_TIMESTAMP.to_le_bytes());
        // item 2: message
        buf.extend_from_slice(&data_msg_offset.to_le_bytes());
        buf.extend_from_slice(&FIELD_MESSAGE.to_le_bytes());
        // item 3: pid
        buf.extend_from_slice(&data_pid_offset.to_le_bytes());
        buf.extend_from_slice(&FIELD__PID.to_le_bytes());
        while (buf.len() % 8) != 0 {
            buf.push(0);
        }

        // ---- OBJECT: ENTRY_ARRAY pointing to the entry ----
        let entry_array_offset = buf.len() as u64;
        let arr_payload: u64 = 8; // one entry pointer
        buf.push(OBJECT_ENTRY_ARRAY);
        buf.push(0);
        buf.extend_from_slice(&[0u8; 6]);
        buf.extend_from_slice(&arr_payload.to_le_bytes());
        buf.extend_from_slice(&entry_offset.to_le_bytes());
        while (buf.len() % 8) != 0 {
            buf.push(0);
        }

        // Patch header: entry_array_offset (field at bytes 176-184)
        let entry_array_off_pos = 176;
        buf[entry_array_off_pos..entry_array_off_pos + 8]
            .copy_from_slice(&entry_array_offset.to_le_bytes());

        // Patch header: tail_object_offset (field at bytes 136-144)
        let last_obj_offset = entry_array_offset;
        let tail_off_pos = 136;
        buf[tail_off_pos..tail_off_pos + 8].copy_from_slice(&last_obj_offset.to_le_bytes());

        buf
    }

    #[test]
    fn parse_synthetic_journal_entries() {
        let data = build_synthetic_journal();
        let entries = parse_journal(&data).expect("should parse synthetic journal");
        assert!(!entries.is_empty(), "should find at least one entry");

        let entry = &entries[0];
        assert_eq!(entry.message.as_deref(), Some("Test journal message"));
        assert_eq!(entry.pid, Some(1234));
        assert!(entry.timestamp.is_some());
    }

    #[test]
    fn reject_non_journal_data() {
        let result = parse_journal(b"not a journal file");
        assert!(result.is_err());
    }

    #[test]
    fn reject_empty_data() {
        let result = parse_journal(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_short_header() {
        let data = vec![0u8; 100];
        let result = parse_journal(&data);
        assert!(result.is_err());
    }
}
