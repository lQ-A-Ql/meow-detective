//! Integration tests for the systemd journal parser using spec-correct
//! synthetic fixtures (see `tests/common/mod.rs`).

mod common;

use artifacts_linux::journal::hash::{jenkins_hash64, siphash24};
use artifacts_linux::{parse_journal, parse_journal_full, LinuxArtifactError};

use common::{
    base_spec, build_journal, entry_array_offset, DataSpec, INCOMPATIBLE_KEYED_HASH, STATE_ONLINE,
};

// ── Known-answer tests for the ported hash functions ───────────────────────

#[test]
fn jenkins_hash64_matches_lookup3_reference_vectors() {
    // From the driver5() self-test in Bob Jenkins' lookup3.c (also shipped
    // in systemd's src/libsystemd/sd-journal/lookup3.c).
    assert_eq!(jenkins_hash64(b""), 0xdead_beef_dead_beef);
    assert_eq!(
        jenkins_hash64(b"Four score and seven years ago"),
        0x1777_0551_ce72_26e6
    );
}

#[test]
fn siphash24_matches_reference_vectors() {
    // Reference SipHash-2-4 vectors (key = 00 01 .. 0f), output read as LE u64.
    let key: [u8; 16] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];
    assert_eq!(siphash24(b"", &key), 0x726f_db47_dd0e_0e31);
    assert_eq!(siphash24(&[0x00], &key), 0x74f8_39c5_93dc_67fd);
}

// ── Happy paths ────────────────────────────────────────────────────────────

#[test]
fn parses_regular_jenkins_file() {
    let data = build_journal(&base_spec());
    let outcome = parse_journal_full(&data).expect("should parse synthetic journal");

    assert_eq!(outcome.entries.len(), 2);
    assert_eq!(outcome.hash_mismatches, 0);
    assert_eq!(outcome.skipped_corrupt, 0);
    assert_eq!(outcome.skipped_compressed, 0);
    assert!(!outcome.truncated);
    assert!(!outcome.entry_limit_hit);

    let entry = &outcome.entries[0];
    assert_eq!(entry.message.as_deref(), Some("Test journal message"));
    assert_eq!(entry.pid, Some(1234));
    assert_eq!(entry.uid, Some(1000));
    assert_eq!(entry.priority, Some(3));
    assert_eq!(entry.systemd_unit.as_deref(), Some("sshd.service"));
    assert_eq!(entry.hostname.as_deref(), Some("testhost"));
    assert_eq!(entry.executable.as_deref(), Some("/usr/sbin/sshd"));
    assert_eq!(entry.cmdline.as_deref(), Some("sshd: alice [priv]"));
    assert_eq!(entry.syslog_identifier.as_deref(), Some("sshd"));
    assert_eq!(
        entry.message_id.as_deref(),
        Some("0123456789abcdef0123456789abcdef")
    );
    assert_eq!(
        entry.selinux_context.as_deref(),
        Some("system_u:system_r:sshd_t:s0")
    );
    assert_eq!(
        entry.boot_id.as_deref(),
        Some("abababababababababababababababab")
    );
    // Timestamp comes from the ENTRY object realtime field.
    assert_eq!(
        entry.timestamp.map(|ts| ts.timestamp_micros()),
        Some(1_700_000_000_000_000)
    );
    assert_eq!(entry.seqnum, Some(1));
    assert_eq!(entry.monotonic, Some(42_000_000));
    // Unknown fields land in raw_fields.
    assert_eq!(
        entry.raw_fields.get("CUSTOM_FIELD").map(String::as_str),
        Some("custom-value")
    );

    let second = &outcome.entries[1];
    assert_eq!(second.message.as_deref(), Some("Second message"));
    assert_eq!(second.priority, Some(6));
    assert_eq!(second.pid, Some(1234));
}

#[test]
fn parses_keyed_hash_file() {
    let mut spec = base_spec();
    spec.incompatible_flags |= INCOMPATIBLE_KEYED_HASH;
    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse keyed-hash journal");

    assert_eq!(outcome.entries.len(), 2);
    assert_eq!(outcome.hash_mismatches, 0);
    assert_eq!(
        outcome.entries[0].message.as_deref(),
        Some("Test journal message")
    );
}

#[test]
fn parses_compact_file() {
    let mut spec = base_spec();
    spec.compact = true;
    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse compact journal");

    assert_eq!(outcome.entries.len(), 2);
    assert_eq!(
        outcome.entries[0].message.as_deref(),
        Some("Test journal message")
    );
    assert_eq!(outcome.entries[0].pid, Some(1234));
    assert_eq!(
        outcome.entries[1].message.as_deref(),
        Some("Second message")
    );
}

#[test]
fn timestamp_falls_back_to_realtime_field() {
    let mut spec = base_spec();
    spec.data
        .push(DataSpec::plain("__REALTIME_TIMESTAMP=1699999999000000"));
    let field_index = spec.data.len() - 1;
    spec.entries[0].realtime = 0;
    spec.entries[0].items.push(field_index);

    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse");
    assert_eq!(
        outcome.entries[0].timestamp.map(|ts| ts.timestamp_micros()),
        Some(1_699_999_999_000_000)
    );
}

// ── Compression ────────────────────────────────────────────────────────────

#[test]
fn lz4_compressed_field_roundtrip() {
    let mut spec = base_spec();
    let long_message = format!("MESSAGE={}", "lz4-payload ".repeat(64));
    spec.data[0] = DataSpec::lz4(&long_message);

    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse");
    assert_eq!(outcome.skipped_compressed, 0);
    assert_eq!(outcome.hash_mismatches, 0);
    assert_eq!(
        outcome.entries[0].message.as_deref(),
        Some(long_message.strip_prefix("MESSAGE=").unwrap_or_default())
    );
}

#[test]
fn zstd_compressed_field_roundtrip() {
    let mut spec = base_spec();
    let long_message = format!("MESSAGE={}", "zstd-payload ".repeat(64));
    spec.data[0] = DataSpec::zstd(&long_message);

    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse");
    assert_eq!(outcome.skipped_compressed, 0);
    assert_eq!(outcome.hash_mismatches, 0);
    assert_eq!(
        outcome.entries[0].message.as_deref(),
        Some(long_message.strip_prefix("MESSAGE=").unwrap_or_default())
    );
}

#[test]
fn xz_compressed_field_roundtrip() {
    let mut spec = base_spec();
    // Long messages (crash stacks etc.) are what XZ actually carries on
    // systemd v219 (RHEL7/CentOS7), where XZ is the default.
    let long_message = format!("MESSAGE={}", "xz-payload ".repeat(256));
    spec.data[0] = DataSpec::xz(&long_message);

    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse");
    assert_eq!(outcome.skipped_compressed, 0);
    assert_eq!(outcome.hash_mismatches, 0);
    assert_eq!(
        outcome.entries[0].message.as_deref(),
        Some(long_message.strip_prefix("MESSAGE=").unwrap_or_default())
    );
}

#[test]
fn mixed_lz4_xz_zstd_file_decodes_all() {
    let mut spec = base_spec();
    let xz_message = format!("MESSAGE={}", "xz-crash-stack ".repeat(64));
    let lz4_message = format!("MESSAGE={}", "lz4-payload ".repeat(64));
    spec.data[0] = DataSpec::xz(&xz_message);
    spec.data[13] = DataSpec::lz4(&lz4_message);
    // Entry 1's PRIORITY arrives Zstd-compressed.
    spec.data[14] = DataSpec::zstd("PRIORITY=6");

    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse");
    assert_eq!(outcome.entries.len(), 2);
    assert_eq!(outcome.skipped_compressed, 0);
    assert_eq!(outcome.hash_mismatches, 0);
    assert_eq!(
        outcome.entries[0].message.as_deref(),
        Some(xz_message.strip_prefix("MESSAGE=").unwrap_or_default())
    );
    assert_eq!(
        outcome.entries[1].message.as_deref(),
        Some(lz4_message.strip_prefix("MESSAGE=").unwrap_or_default())
    );
    assert_eq!(outcome.entries[1].priority, Some(6));
}

#[test]
fn corrupt_xz_blob_is_counted_not_fatal() {
    let mut spec = base_spec();
    // Claim XZ compression but store garbage instead of a container.
    spec.data[0] = DataSpec::raw(common::FLAG_XZ, vec![0xFF; 64]);

    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse");
    assert_eq!(outcome.entries.len(), 2);
    assert!(outcome.entries[0].message.is_none());
    assert!(outcome.skipped_compressed >= 1);
}

#[test]
fn xz_blob_exceeding_output_cap_is_counted() {
    let mut spec = base_spec();
    // A valid XZ stream decompressing past the 64 MiB cap must be dropped.
    // Build it directly: zeros compress to well under a megabyte.
    let huge = vec![b'A'; (65 * 1024 * 1024) as usize];
    let stream = xz2::stream::Stream::new_easy_encoder(6, xz2::stream::Check::None)
        .expect("XZ encoder preset must be valid");
    let mut encoder = xz2::write::XzEncoder::new_stream(Vec::new(), stream);
    use std::io::Write as _;
    encoder.write_all(&huge).expect("encode");
    let stored = encoder.finish().expect("encode");
    spec.data[0] = DataSpec::raw(common::FLAG_XZ, stored);

    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse");
    assert!(outcome.entries[0].message.is_none());
    assert!(outcome.skipped_compressed >= 1);
}

#[test]
fn corrupt_compressed_payload_is_counted_not_fatal() {
    let mut spec = base_spec();
    // Claim LZ4 compression but store garbage after the size prefix.
    let mut payload = (128u64).to_le_bytes().to_vec();
    payload.extend_from_slice(&[0xFF; 32]);
    spec.data[0] = DataSpec::raw(common::FLAG_LZ4, payload);

    let data = build_journal(&spec);
    let outcome = parse_journal_full(&data).expect("should parse");
    assert!(outcome.entries[0].message.is_none());
    assert!(outcome.skipped_compressed >= 1);
}

// ── Robustness / negative cases ────────────────────────────────────────────

#[test]
fn truncated_online_file_is_tolerated() {
    let mut spec = base_spec();
    spec.state = STATE_ONLINE;
    let mut data = build_journal(&spec);
    let original_len = data.len();
    data.truncate(original_len - 24); // cut the ENTRY_ARRAY tail

    let outcome = parse_journal_full(&data).expect("truncated file must not error");
    assert!(outcome.truncated);
    // The fallback linear scan still finds both complete ENTRY objects.
    assert_eq!(outcome.entries.len(), 2);
}

#[test]
fn forged_giant_object_size_neither_panics_nor_hangs() {
    let spec = base_spec();
    let mut data = build_journal(&spec);
    let entry_array = entry_array_offset(&data) as usize;
    // Forge an astronomical size on the ENTRY_ARRAY object header.
    data[entry_array + 8..entry_array + 16].copy_from_slice(&u64::MAX.to_le_bytes());

    let outcome = parse_journal_full(&data).expect("forged size must not error");
    // Chain walk rejects the forged object; linear scan still finds entries.
    assert_eq!(outcome.entries.len(), 2);
}

#[test]
fn cyclic_entry_array_chain_falls_back_to_linear_scan() {
    let spec = base_spec();
    let mut data = build_journal(&spec);
    let entry_array = entry_array_offset(&data) as usize;
    // Point next_entry_array_offset at the ENTRY_ARRAY itself.
    let self_offset = entry_array as u64;
    data[entry_array + 16..entry_array + 24].copy_from_slice(&self_offset.to_le_bytes());

    let outcome = parse_journal_full(&data).expect("cycle must not hang");
    assert_eq!(outcome.entries.len(), 2);
}

#[test]
fn bad_entry_array_offset_falls_back_to_linear_scan() {
    let spec = base_spec();
    let mut data = build_journal(&spec);
    // Point entry_array_offset at the middle of nowhere (aligned, in-bounds).
    data[176..184].copy_from_slice(&272u64.to_le_bytes());

    let outcome = parse_journal_full(&data).expect("bad offset must not error");
    assert_eq!(outcome.entries.len(), 2);
}

#[test]
fn unknown_incompatible_flag_is_unsupported() {
    let spec = base_spec();
    let mut data = build_journal(&spec);
    let flags = 1u32 << 20;
    data[12..16].copy_from_slice(&flags.to_le_bytes());

    let error = parse_journal(&data).expect_err("unknown flags must be rejected");
    assert!(matches!(error, LinuxArtifactError::Unsupported { .. }));
}

#[test]
fn entry_pointing_outside_arena_is_skipped() {
    let spec = base_spec();
    let mut data = build_journal(&spec);
    let entry_array = entry_array_offset(&data) as usize;
    // Second array slot points past the end of the file.
    let bogus = (data.len() as u64 + 4096) & !7;
    data[entry_array + 24..entry_array + 32].copy_from_slice(&bogus.to_le_bytes());

    let outcome = parse_journal_full(&data).expect("bad entry offset must not error");
    assert_eq!(outcome.entries.len(), 1);
    assert!(outcome.skipped_corrupt >= 1);
}

// ── Rejection of non-journal input ─────────────────────────────────────────

#[test]
fn reject_non_journal_data() {
    assert!(parse_journal(b"not a journal file").is_err());
}

#[test]
fn reject_empty_data() {
    assert!(parse_journal(&[]).is_err());
}

#[test]
fn reject_short_header() {
    let data = vec![0u8; 100];
    assert!(parse_journal(&data).is_err());
}
