//! Shared, spec-correct synthetic systemd journal fixture builder.
//!
//! Layout follows https://systemd.io/JOURNAL_FILE_FORMAT/ and systemd's
//! `journal-def.h` / `compress.c`:
//! - 272-byte header (current `sizeof(struct Header)`), all integers LE.
//! - FIELD objects: hash + next_hash_offset + head_data_offset + name (no `=`).
//! - DATA objects: hash + next_hash_offset + next_field_offset + entry_offset
//!   plus entry_array_offset + n_entries (+ two le32 tail fields when
//!   COMPACT), then the `FIELD=value` payload, optionally compressed.
//! - ENTRY objects: seqnum + realtime + monotonic + boot_id + xor_hash,
//!   then items (`{ le64 offset, le64 hash }`, or a single le32 offset when
//!   COMPACT).
//! - ENTRY_ARRAY objects: next_entry_array_offset + le64/le32 item offsets.
//!
//! Payload hashes are computed with the same hash the format mandates:
//! Jenkins lookup3 64-bit for regular files, SipHash-2-4 keyed by `file_id`
//! for `HEADER_INCOMPATIBLE_KEYED_HASH` files. `xor_hash` is always Jenkins.
#![allow(dead_code)]

use artifacts_linux::journal::hash::{jenkins_hash64, siphash24};

pub const STATE_OFFLINE: u8 = 0;
pub const STATE_ONLINE: u8 = 1;

pub const INCOMPATIBLE_COMPRESSED_XZ: u32 = 1 << 0;
pub const INCOMPATIBLE_COMPRESSED_LZ4: u32 = 1 << 1;
pub const INCOMPATIBLE_KEYED_HASH: u32 = 1 << 2;
pub const INCOMPATIBLE_COMPRESSED_ZSTD: u32 = 1 << 3;
pub const INCOMPATIBLE_COMPACT: u32 = 1 << 4;

pub const FLAG_XZ: u8 = 1 << 0;
pub const FLAG_LZ4: u8 = 1 << 1;
pub const FLAG_ZSTD: u8 = 1 << 2;

const HEADER_SIZE: u64 = 272;

/// One DATA object: the plaintext `FIELD=value` payload plus the
/// `ObjectHeader.flags` compression bits the object carries.
pub struct DataSpec {
    pub flags: u8,
    pub payload: Vec<u8>,
    /// When true, `payload` is stored verbatim (already-encoded bytes) even
    /// if `flags` claim compression — used to forge corrupt compressed blobs.
    pub precompressed: bool,
}

impl DataSpec {
    pub fn plain(field_value: &str) -> Self {
        Self {
            flags: 0,
            payload: field_value.as_bytes().to_vec(),
            precompressed: false,
        }
    }

    /// LZ4-compressed DATA payload: le64 original size + raw LZ4 block,
    /// exactly as written by systemd's `compress_blob_lz4`.
    pub fn lz4(field_value: &str) -> Self {
        Self {
            flags: FLAG_LZ4,
            payload: field_value.as_bytes().to_vec(),
            precompressed: false,
        }
    }

    pub fn zstd(field_value: &str) -> Self {
        Self {
            flags: FLAG_ZSTD,
            payload: field_value.as_bytes().to_vec(),
            precompressed: false,
        }
    }

    /// XZ-flagged payload. Decompression is unsupported, so the body is
    /// irrelevant; the parser must skip and count it.
    pub fn xz(field_value: &str) -> Self {
        Self {
            flags: FLAG_XZ,
            payload: field_value.as_bytes().to_vec(),
            precompressed: true,
        }
    }

    /// DATA object with compression flags but caller-supplied stored bytes.
    pub fn raw(flags: u8, stored: Vec<u8>) -> Self {
        Self {
            flags,
            payload: stored,
            precompressed: true,
        }
    }
}

pub struct EntrySpec {
    pub seqnum: u64,
    /// Microseconds since the Unix epoch; 0 means "no timestamp".
    pub realtime: u64,
    pub monotonic: u64,
    pub boot_id: [u8; 16],
    /// Indices into `JournalSpec::data`.
    pub items: Vec<usize>,
}

pub struct JournalSpec {
    pub incompatible_flags: u32,
    pub state: u8,
    pub file_id: [u8; 16],
    pub compact: bool,
    pub fields: Vec<String>,
    pub data: Vec<DataSpec>,
    pub entries: Vec<EntrySpec>,
}

impl Default for JournalSpec {
    fn default() -> Self {
        Self {
            incompatible_flags: 0,
            state: STATE_OFFLINE,
            file_id: [0x11; 16],
            compact: false,
            fields: Vec::new(),
            data: Vec::new(),
            entries: Vec::new(),
        }
    }
}

/// Two-entry baseline fixture used by both the journal integration tests and
/// the cross-parser integration test.
pub fn base_spec() -> JournalSpec {
    let boot_id = [0xAB; 16];
    JournalSpec {
        fields: vec!["MESSAGE".to_string(), "_PID".to_string()],
        data: vec![
            DataSpec::plain("MESSAGE=Test journal message"),
            DataSpec::plain("PRIORITY=3"),
            DataSpec::plain("_PID=1234"),
            DataSpec::plain("_UID=1000"),
            DataSpec::plain("_SYSTEMD_UNIT=sshd.service"),
            DataSpec::plain("_HOSTNAME=testhost"),
            DataSpec::plain("_EXE=/usr/sbin/sshd"),
            DataSpec::plain("_CMDLINE=sshd: alice [priv]"),
            DataSpec::plain("SYSLOG_IDENTIFIER=sshd"),
            DataSpec::plain("MESSAGE_ID=0123456789abcdef0123456789abcdef"),
            DataSpec::plain("_SELINUX_CONTEXT=system_u:system_r:sshd_t:s0"),
            DataSpec::plain("_BOOT_ID=abababababababababababababababab"),
            DataSpec::plain("CUSTOM_FIELD=custom-value"),
            DataSpec::plain("MESSAGE=Second message"),
            DataSpec::plain("PRIORITY=6"),
        ],
        entries: vec![
            EntrySpec {
                seqnum: 1,
                realtime: 1_700_000_000_000_000,
                monotonic: 42_000_000,
                boot_id,
                items: vec![0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            },
            EntrySpec {
                seqnum: 2,
                realtime: 1_700_000_001_000_000,
                monotonic: 43_000_000,
                boot_id,
                items: vec![13, 14, 2, 4],
            },
        ],
        ..JournalSpec::default()
    }
}

/// Build the binary journal file for `spec`.
pub fn build_journal(spec: &JournalSpec) -> Vec<u8> {
    let keyed = spec.incompatible_flags & INCOMPATIBLE_KEYED_HASH != 0;
    let hash = |payload: &[u8]| -> u64 {
        if keyed {
            siphash24(payload, &spec.file_id)
        } else {
            jenkins_hash64(payload)
        }
    };

    let mut incompatible = spec.incompatible_flags;
    if spec.compact {
        incompatible |= INCOMPATIBLE_COMPACT;
    }
    for data in &spec.data {
        incompatible |= match data.flags {
            FLAG_XZ => INCOMPATIBLE_COMPRESSED_XZ,
            FLAG_LZ4 => INCOMPATIBLE_COMPRESSED_LZ4,
            FLAG_ZSTD => INCOMPATIBLE_COMPRESSED_ZSTD,
            _ => 0,
        };
    }

    let mut buf = build_header(spec, incompatible);
    let data_offsets = push_data_objects(&mut buf, spec, hash);
    let entry_offsets = push_entry_objects(&mut buf, spec, &data_offsets, hash);
    let entry_array_offset = push_entry_array(&mut buf, spec, &entry_offsets);
    patch_header(&mut buf, spec, entry_array_offset);
    buf
}

/// Offset of the first ENTRY_ARRAY object, read back from a built file.
pub fn entry_array_offset(buf: &[u8]) -> u64 {
    u64::from_le_bytes(buf[176..184].try_into().unwrap_or([0; 8]))
}

fn build_header(spec: &JournalSpec, incompatible: u32) -> Vec<u8> {
    let mut buf = Vec::with_capacity(HEADER_SIZE as usize);
    buf.extend_from_slice(b"LPKSHHRH");
    buf.extend_from_slice(&0u32.to_le_bytes()); // compatible_flags
    buf.extend_from_slice(&incompatible.to_le_bytes());
    buf.push(spec.state);
    buf.extend_from_slice(&[0u8; 7]); // reserved
    buf.extend_from_slice(&spec.file_id);
    buf.extend_from_slice(&[0x22; 16]); // machine_id
    buf.extend_from_slice(&[0xAB; 16]); // tail_entry_boot_id
    buf.extend_from_slice(&[0x33; 16]); // seqnum_id
    buf.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    buf.extend_from_slice(&0u64.to_le_bytes()); // arena_size (patched)
    buf.extend_from_slice(&[0u8; 8 * 4]); // hash table offsets/sizes
    buf.extend_from_slice(&0u64.to_le_bytes()); // tail_object_offset (patched)
    buf.extend_from_slice(&0u64.to_le_bytes()); // n_objects (patched)
    buf.extend_from_slice(&(spec.entries.len() as u64).to_le_bytes()); // n_entries
    buf.extend_from_slice(&[0u8; 8 * 2]); // tail/head_entry_seqnum
    buf.extend_from_slice(&0u64.to_le_bytes()); // entry_array_offset (patched)
    buf.extend_from_slice(&[0u8; 8 * 3]); // head/tail realtime, tail monotonic
    buf.extend_from_slice(&(spec.data.len() as u64).to_le_bytes()); // n_data
    buf.extend_from_slice(&(spec.fields.len() as u64).to_le_bytes()); // n_fields
    buf.extend_from_slice(&0u64.to_le_bytes()); // n_tags
    buf.extend_from_slice(&1u64.to_le_bytes()); // n_entry_arrays
    buf.extend_from_slice(&[0u8; 8 * 2]); // hash chain depths (v246)
    buf.extend_from_slice(&[0u8; 4 * 2]); // tail_entry_array_offset/n_entries (v252)
    buf.extend_from_slice(&[0u8; 8]); // tail_entry_offset (v254)
    debug_assert_eq!(buf.len() as u64, HEADER_SIZE);
    buf
}

fn push_object(buf: &mut Vec<u8>, object_type: u8, flags: u8, payload: &[u8]) -> u64 {
    while !buf.len().is_multiple_of(8) {
        buf.push(0);
    }
    let offset = buf.len() as u64;
    buf.push(object_type);
    buf.push(flags);
    buf.extend_from_slice(&[0u8; 6]); // reserved
    buf.extend_from_slice(&(16 + payload.len() as u64).to_le_bytes());
    buf.extend_from_slice(payload);
    offset
}

fn push_data_objects(
    buf: &mut Vec<u8>,
    spec: &JournalSpec,
    hash: impl Fn(&[u8]) -> u64,
) -> Vec<u64> {
    for name in &spec.fields {
        let mut payload = Vec::new();
        payload.extend_from_slice(&hash(name.as_bytes()).to_le_bytes());
        payload.extend_from_slice(&[0u8; 8 * 2]); // next_hash_offset, head_data_offset
        payload.extend_from_slice(name.as_bytes());
        push_object(buf, 2, 0, &payload);
    }

    let mut reference_counts = vec![0u64; spec.data.len()];
    for entry in &spec.entries {
        for index in &entry.items {
            reference_counts[*index] += 1;
        }
    }

    let mut offsets = Vec::new();
    for (index, data) in spec.data.iter().enumerate() {
        let mut payload = Vec::new();
        payload.extend_from_slice(&hash(&data.payload).to_le_bytes());
        payload.extend_from_slice(&[0u8; 8 * 4]); // hash chain / field / entry links
        payload.extend_from_slice(&reference_counts[index].to_le_bytes());
        if spec.compact {
            payload.extend_from_slice(&[0u8; 4 * 2]); // tail entry array fields
        }
        payload.extend_from_slice(&stored_payload(data));
        offsets.push(push_object(buf, 1, data.flags, &payload));
    }
    offsets
}

fn stored_payload(data: &DataSpec) -> Vec<u8> {
    if data.precompressed {
        return data.payload.clone();
    }
    match data.flags {
        FLAG_LZ4 => {
            let mut stored = (data.payload.len() as u64).to_le_bytes().to_vec();
            stored.extend_from_slice(&lz4_flex::block::compress(&data.payload));
            stored
        }
        FLAG_ZSTD => {
            zstd::stream::encode_all(std::io::Cursor::new(&data.payload), 0).unwrap_or_default()
        }
        _ => data.payload.clone(),
    }
}

fn push_entry_objects(
    buf: &mut Vec<u8>,
    spec: &JournalSpec,
    data_offsets: &[u64],
    hash: impl Fn(&[u8]) -> u64,
) -> Vec<u64> {
    let mut offsets = Vec::new();
    for entry in &spec.entries {
        // The xor_hash always uses the Jenkins hash, even for keyed files.
        let xor_hash = entry.items.iter().fold(0u64, |acc, index| {
            acc ^ jenkins_hash64(&spec.data[*index].payload)
        });

        let mut payload = Vec::new();
        payload.extend_from_slice(&entry.seqnum.to_le_bytes());
        payload.extend_from_slice(&entry.realtime.to_le_bytes());
        payload.extend_from_slice(&entry.monotonic.to_le_bytes());
        payload.extend_from_slice(&entry.boot_id);
        payload.extend_from_slice(&xor_hash.to_le_bytes());
        for index in &entry.items {
            if spec.compact {
                payload.extend_from_slice(&(data_offsets[*index] as u32).to_le_bytes());
            } else {
                payload.extend_from_slice(&data_offsets[*index].to_le_bytes());
                let item_hash = hash(&spec.data[*index].payload);
                payload.extend_from_slice(&item_hash.to_le_bytes());
            }
        }
        offsets.push(push_object(buf, 3, 0, &payload));
    }
    offsets
}

fn push_entry_array(buf: &mut Vec<u8>, spec: &JournalSpec, entry_offsets: &[u64]) -> u64 {
    let mut payload = Vec::new();
    payload.extend_from_slice(&0u64.to_le_bytes()); // next_entry_array_offset
    for offset in entry_offsets {
        if spec.compact {
            payload.extend_from_slice(&(*offset as u32).to_le_bytes());
        } else {
            payload.extend_from_slice(&offset.to_le_bytes());
        }
    }
    push_object(buf, 6, 0, &payload)
}

fn patch_header(buf: &mut [u8], spec: &JournalSpec, entry_array_offset: u64) {
    let arena_size = buf.len() as u64 - HEADER_SIZE;
    buf[96..104].copy_from_slice(&arena_size.to_le_bytes());
    buf[136..144].copy_from_slice(&entry_array_offset.to_le_bytes()); // tail_object_offset
    let n_objects = (spec.fields.len() + spec.data.len() + spec.entries.len() + 1) as u64;
    buf[144..152].copy_from_slice(&n_objects.to_le_bytes());
    buf[176..184].copy_from_slice(&entry_array_offset.to_le_bytes());
}
