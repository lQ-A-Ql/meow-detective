use super::reader::RegistryHiveReader;
use super::txlog_util::apply_single_txlog_override;
use super::*;

// ── SYSTEM hive field extraction ──────────────────────────────────────────────

pub fn extract_system_hive_fields(bytes: &[u8], hive_path: &str) -> Result<SystemHiveInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut info = SystemHiveInfo::default();
    let control_sets = hive.control_set_candidates(&mut info.warnings);

    for control_set in control_sets {
        let computer_key = [
            control_set.as_str(),
            "Control",
            "ComputerName",
            "ComputerName",
        ];
        if info.computer_name.is_none() {
            info.computer_name = lookup_string_field(
                &hive,
                hive_path,
                "registry.system",
                &computer_key,
                "ComputerName",
                &mut info.warnings,
            );
        }

        let timezone_key = [control_set.as_str(), "Control", "TimeZoneInformation"];
        if info.timezone.is_none() {
            info.timezone = lookup_string_field(
                &hive,
                hive_path,
                "registry.system",
                &timezone_key,
                "TimeZoneKeyName",
                &mut info.warnings,
            )
            .or_else(|| {
                lookup_string_field(
                    &hive,
                    hive_path,
                    "registry.system",
                    &timezone_key,
                    "StandardName",
                    &mut info.warnings,
                )
            });
        }

        if info.computer_name.is_some() && info.timezone.is_some() {
            break;
        }
    }
    Ok(info)
}

/// Like [`extract_system_hive_fields`], but after standard extraction checks a
/// transaction log for more recent writes.  When a txlog entry holds a newer
/// value (higher sequence number), the field's value is overwritten.
pub fn extract_system_hive_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<SystemHiveInfo, String> {
    let mut info = extract_system_hive_fields(bytes, hive_path)?;
    let txlog = parse_transaction_log(txlog_data)?;
    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    if let Some(ref mut field) = info.computer_name {
        let ts = apply_single_txlog_override(field, &txlog.transactions);
        txlog_applied = txlog_applied || ts.txlog_used;
        ts_infos.push(ts);
    }
    if let Some(ref mut field) = info.timezone {
        let ts = apply_single_txlog_override(field, &txlog.transactions);
        txlog_applied = txlog_applied || ts.txlog_used;
        ts_infos.push(ts);
    }

    info.txlog_applied = txlog_applied;
    info.txlog_timestamps = ts_infos;
    Ok(info)
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::test_common::*;
    use super::*;
    use testing::{builders::registry as registry_fixture, fixtures};

    #[test]
    fn extract_system_fields_from_fixture() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "Control",
            &[("ComputerName", 0x600), ("TimeZoneInformation", 0xa00)],
            &[],
        );
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", "LAB-PC", 0x1800);
        write_nk(&mut data, 0xa00, "TimeZoneInformation", &[], &[0xd00]);
        write_string_value(
            &mut data,
            0xd00,
            "TimeZoneKeyName",
            "China Standard Time",
            0x1900,
        );

        let info = extract_system_hive_fields(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(info.computer_name.unwrap().value, "LAB-PC");
        assert_eq!(info.timezone.unwrap().value, "China Standard Time");
    }

    #[test]
    fn extract_system_fields_falls_back_when_select_is_corrupt() {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_vk(
            &mut data,
            0x1200,
            "Current",
            REG_DWORD,
            0x8000_0004,
            0x9530_7897,
        );
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("ComputerName", 0x600)], &[]);
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", "LAB-PC", 0x1800);

        let info = extract_system_hive_fields(&data, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(info.computer_name.unwrap().value, "LAB-PC");
        assert!(info
            .warnings
            .iter()
            .any(|warning| warning.contains("Select\\Current")));
    }

    #[test]
    fn extract_system_fields_from_committed_tiny_fixture() {
        let bytes = std::fs::read(fixtures::tiny_registry_system_hive())
            .expect("read tiny SYSTEM registry fixture");

        let info = extract_system_hive_fields(&bytes, "Windows/System32/config/SYSTEM").unwrap();

        assert_eq!(
            info.computer_name
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SYSTEM_COMPUTER_NAME)
        );
        assert_eq!(
            info.timezone.as_ref().map(|field| field.value.as_str()),
            Some(registry_fixture::SYSTEM_TIMEZONE)
        );
        assert!(info.warnings.is_empty());
    }

    // ── Txlog-override tests ───────────────────────────────────────────────

    use crate::registry::txlog::fixture::{build_synthetic_log1, SyntheticEntry};

    /// Build a minimal synthetic SYSTEM hive that has a ComputerName value.
    fn txlog_system_hive(computer_name: &str) -> Vec<u8> {
        let mut data = empty_hive("SYSTEM");
        write_nk(
            &mut data,
            0x20,
            "SYSTEM",
            &[("Select", 0x200), ("ControlSet001", 0x300)],
            &[],
        );
        write_nk(&mut data, 0x200, "Select", &[], &[0x1200]);
        write_dword_value(&mut data, 0x1200, "Current", 1);
        write_nk(
            &mut data,
            0x300,
            "ControlSet001",
            &[("Control", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "Control", &[("ComputerName", 0x600)], &[]);
        write_nk(
            &mut data,
            0x600,
            "ComputerName",
            &[("ComputerName", 0x800)],
            &[],
        );
        write_nk(&mut data, 0x800, "ComputerName", &[], &[0xc00]);
        write_string_value(&mut data, 0xc00, "ComputerName", computer_name, 0x1800);
        data
    }

    #[test]
    fn system_hive_with_txlog_overrides_computer_name() {
        let hive_bytes = txlog_system_hive("OLD-PC");

        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 100,
            timestamp: Some(0x01DB_9F8C_0000_0000), // 2026-06-14 approx
            key_path:
                "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName"
                    .to_string(),
            value_name: Some("ComputerName".to_string()),
            data_before: Some(encode_utf16le("OLD-PC")),
            data_after: Some(encode_utf16le("NEW-PC")),
        }]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        let cn = info.computer_name.as_ref().unwrap();
        assert_eq!(
            cn.value, "NEW-PC",
            "ComputerName should be overridden by txlog"
        );
        assert!(info.txlog_applied, "txlog_applied should be true");
        assert_eq!(info.txlog_timestamps.len(), 1);
        let ts = &info.txlog_timestamps[0];
        assert_eq!(ts.field_name, "ComputerName");
        assert!(ts.txlog_used);
        assert!(ts.txlog_timestamp.is_some());
        assert!(ts.hive_timestamp.is_none());
    }

    #[test]
    fn system_hive_with_txlog_no_match_leaves_field_unchanged() {
        let hive_bytes = txlog_system_hive("ORIGINAL-PC");

        // Txlog entry for a completely different key — should not match.
        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 1,
            timestamp: Some(0x01DB_9F8C_0000_0000),
            key_path: "\\Registry\\Machine\\SOFTWARE\\Some\\Other\\Path".to_string(),
            value_name: Some("Unrelated".to_string()),
            data_before: None,
            data_after: Some(encode_utf16le("ignored")),
        }]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        let cn = info.computer_name.as_ref().unwrap();
        assert_eq!(
            cn.value, "ORIGINAL-PC",
            "ComputerName should stay unchanged"
        );
        assert!(!info.txlog_applied);
        let ts = &info.txlog_timestamps[0];
        assert_eq!(ts.field_name, "ComputerName");
        assert!(!ts.txlog_used);
        assert!(ts.txlog_timestamp.is_none());
    }

    #[test]
    fn txlog_uses_highest_sequence_number() {
        // When multiple txlog entries match the same field, use the one with
        // the highest sequence number.
        let hive_bytes = txlog_system_hive("V1");

        let txlog_bytes = build_synthetic_log1(&[
            SyntheticEntry {
                operation: 2,
                sequence_number: 10,
                timestamp: Some(0x01DB_9F8C_0000_0000),
                key_path: "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
                value_name: Some("ComputerName".to_string()),
                data_before: Some(encode_utf16le("V1")),
                data_after: Some(encode_utf16le("V2")),
            },
            SyntheticEntry {
                operation: 2,
                sequence_number: 20, // higher seq → should win
                timestamp: Some(0x01DB_A000_0000_0000),
                key_path: "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName".to_string(),
                value_name: Some("ComputerName".to_string()),
                data_before: Some(encode_utf16le("V2")),
                data_after: Some(encode_utf16le("V3")),
            },
        ]);

        let info = extract_system_hive_fields_with_txlog(
            &hive_bytes,
            "Windows/System32/config/SYSTEM",
            &txlog_bytes,
        )
        .unwrap();

        assert_eq!(info.computer_name.as_ref().unwrap().value, "V3");
    }
}
