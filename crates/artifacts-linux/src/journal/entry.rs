use std::collections::HashMap;
use std::io::{Cursor, Read};

use chrono::{DateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use super::header::read_le_u64;
use super::object::{decompress_if_needed, ObjectHeader, OBJECT_DATA, OBJECT_ENTRY};

pub(super) const FIELD__REALTIME_TIMESTAMP: u64 = 0;
pub(super) const _FIELD__MONOTONIC_TIMESTAMP: u64 = 1;
pub(super) const FIELD__BOOT_ID: u64 = 2;
pub(super) const FIELD_PRIORITY: u64 = 3;
pub(super) const FIELD__PID: u64 = 4;
pub(super) const FIELD__UID: u64 = 5;
pub(super) const FIELD__GID: u64 = 6;
pub(super) const FIELD__SYSTEMD_UNIT: u64 = 7;
pub(super) const FIELD__SYSTEMD_USER_UNIT: u64 = 8;
pub(super) const FIELD_COMM: u64 = 9;
pub(super) const FIELD_MESSAGE: u64 = 10;
pub(super) const FIELD__EXE: u64 = 11;
pub(super) const FIELD__CMDLINE: u64 = 12;
pub(super) const FIELD__SYSTEMD_CGROUP: u64 = 13;
pub(super) const FIELD__SYSTEMD_SLICE: u64 = 14;
pub(super) const FIELD__TRANSPORT: u64 = 15;
pub(super) const FIELD__HOSTNAME: u64 = 16;
pub(super) const FIELD_SYSLOG_FACILITY: u64 = 17;
pub(super) const FIELD_SYSLOG_IDENTIFIER: u64 = 18;
pub(super) const FIELD_CODE_FILE: u64 = 19;
pub(super) const FIELD_CODE_LINE: u64 = 20;
pub(super) const FIELD_CODE_FUNC: u64 = 21;
pub(super) const FIELD_ERRNO: u64 = 22;
pub(super) const FIELD__SOURCE_REALTIME_TIMESTAMP: u64 = 23;
pub(super) const _FIELD__CURSOR: u64 = 24;
pub(super) const FIELD__AUDIT_LOGINUID: u64 = 25;
pub(super) const FIELD__AUDIT_SESSION: u64 = 26;
pub(super) const FIELD__SELINUX_CONTEXT: u64 = 27;
pub(super) const FIELD_MESSAGE_ID: u64 = 28;
pub(super) const _FIELD_USER_INVOCATION_ID: u64 = 29;
pub(super) const _FIELD_SYSTEMD_INVOCATION_ID: u64 = 30;
pub(super) const _FIELD_N_ENTRY_ARRAYS: u64 = 31;

pub(super) const WELL_KNOWN_FIELDS: &[(&str, u64)] = &[
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
}

pub(super) fn parse_entry_at_offset(
    reader: &mut Cursor<&[u8]>,
    data: &[u8],
    entry_offset: u64,
    has_compressed: bool,
) -> Option<JournalEntry> {
    if entry_offset + 16 > data.len() as u64 {
        return None;
    }

    let save_pos = reader.position();
    reader.set_position(entry_offset);

    let object = match ObjectHeader::read(reader) {
        Ok(header) => header,
        Err(_) => {
            reader.set_position(save_pos);
            return None;
        }
    };

    if object.object_type != OBJECT_ENTRY || object.payload_size == 0 {
        reader.set_position(save_pos);
        return None;
    }

    let count = object.payload_size / 16;
    let mut entry_fields = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let object_offset = match read_le_u64(reader) {
            Ok(value) => value,
            Err(_) => break,
        };
        let field_hash = match read_le_u64(reader) {
            Ok(value) => value,
            Err(_) => break,
        };
        entry_fields.push((object_offset, field_hash));
    }

    let mut entry = JournalEntry::default();
    for (object_offset, field_hash) in entry_fields {
        if let Some(value) = read_data_value(reader, data, object_offset, has_compressed) {
            if let Ok(text) = std::str::from_utf8(&value) {
                fill_journal_entry_field(&mut entry, strip_trailing_newline(text), field_hash);
            }
        }
    }

    reader.set_position(save_pos);

    if entry.message.is_some() || entry.timestamp.is_some() {
        finalize_raw_fields(&mut entry);
        Some(entry)
    } else {
        None
    }
}

fn read_data_value(
    reader: &mut Cursor<&[u8]>,
    data: &[u8],
    object_offset: u64,
    has_compressed: bool,
) -> Option<Vec<u8>> {
    if object_offset + 16 > data.len() as u64 {
        return None;
    }

    let save_pos = reader.position();
    reader.set_position(object_offset);

    let value = (|| {
        let object = ObjectHeader::read(reader).ok()?;
        if object.object_type != OBJECT_DATA || object.payload_size < 8 {
            return None;
        }

        let _data_hash = read_le_u64(reader).ok()?;
        let value_len = (object.payload_size - 8) as usize;
        if value_len == 0 || reader.position() as usize + value_len > data.len() {
            return None;
        }

        let mut raw_value = vec![0u8; value_len];
        reader.read_exact(&mut raw_value).ok()?;
        if has_compressed && !raw_value.is_empty() {
            Some(decompress_if_needed(&raw_value).unwrap_or(raw_value))
        } else {
            Some(raw_value)
        }
    })();

    reader.set_position(save_pos);
    value
}

fn finalize_raw_fields(entry: &mut JournalEntry) {
    if let Some(timestamp) = entry.timestamp {
        entry.raw_fields.insert(
            "__REALTIME_TIMESTAMP".to_string(),
            timestamp.timestamp_micros().to_string(),
        );
    }
    if let Some(ref message) = entry.message {
        entry
            .raw_fields
            .insert("MESSAGE".to_string(), message.clone());
    }
    if let Some(pid) = entry.pid {
        entry.raw_fields.insert("_PID".to_string(), pid.to_string());
    }
    if let Some(uid) = entry.uid {
        entry.raw_fields.insert("_UID".to_string(), uid.to_string());
    }
    if let Some(ref executable) = entry.executable {
        entry
            .raw_fields
            .insert("_EXE".to_string(), executable.clone());
    }
    if let Some(ref boot_id) = entry.boot_id {
        entry
            .raw_fields
            .insert("_BOOT_ID".to_string(), boot_id.clone());
    }
    if let Some(priority) = entry.priority {
        entry
            .raw_fields
            .insert("PRIORITY".to_string(), priority.to_string());
    }
    if let Some(ref unit) = entry.systemd_unit {
        entry
            .raw_fields
            .insert("_SYSTEMD_UNIT".to_string(), unit.clone());
    }
    if let Some(ref host) = entry.hostname {
        entry
            .raw_fields
            .insert("_HOSTNAME".to_string(), host.clone());
    }
    if let Some(ref cmdline) = entry.cmdline {
        entry
            .raw_fields
            .insert("_CMDLINE".to_string(), cmdline.clone());
    }
}

fn fill_journal_entry_field(entry: &mut JournalEntry, value: &str, field_hash: u64) {
    match field_hash {
        FIELD__REALTIME_TIMESTAMP => {
            if let Ok(micros) = value.parse::<i64>() {
                entry.timestamp = Utc.timestamp_micros(micros).single();
            }
        }
        FIELD_MESSAGE => {
            entry.message = Some(value.to_string());
        }
        FIELD__PID => {
            entry.pid = value.parse::<u32>().ok();
        }
        FIELD__UID => {
            entry.uid = value.parse::<u32>().ok();
        }
        FIELD__EXE => {
            entry.executable = Some(value.to_string());
        }
        FIELD_PRIORITY => {
            entry.priority = value.parse::<u8>().ok();
        }
        FIELD__BOOT_ID => {
            entry.boot_id = Some(value.to_string());
        }
        FIELD__SYSTEMD_UNIT => {
            entry.systemd_unit = Some(value.to_string());
        }
        FIELD__HOSTNAME => {
            entry.hostname = Some(value.to_string());
        }
        FIELD__CMDLINE => {
            entry.cmdline = Some(value.to_string());
        }
        FIELD__SELINUX_CONTEXT => {
            entry.selinux_context = Some(value.to_string());
        }
        FIELD_SYSLOG_IDENTIFIER => {
            entry.syslog_identifier = Some(value.to_string());
        }
        FIELD_MESSAGE_ID => {
            entry.message_id = Some(value.to_string());
        }
        _ => {
            if let Some(name) = well_known_field_name(field_hash) {
                entry.raw_fields.insert(name.to_string(), value.to_string());
            }
        }
    }
}

fn well_known_field_name(index: u64) -> Option<&'static str> {
    WELL_KNOWN_FIELDS
        .iter()
        .find(|(_, field_index)| *field_index == index)
        .map(|(name, _)| *name)
}

fn strip_trailing_newline(value: &str) -> &str {
    value.strip_suffix('\n').unwrap_or(value)
}
