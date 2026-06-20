use super::*;

// ── SOFTWARE hive field extraction ────────────────────────────────────────────

pub fn extract_software_hive_fields(
    bytes: &[u8],
    hive_path: &str,
) -> Result<SoftwareHiveInfo, String> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key = ["Microsoft", "Windows NT", "CurrentVersion"];
    let mut info = SoftwareHiveInfo::default();

    info.product_name = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "ProductName",
        &mut info.warnings,
    );
    info.current_build = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "CurrentBuild",
        &mut info.warnings,
    );
    info.current_version = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "CurrentVersion",
        &mut info.warnings,
    );
    info.display_version = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "DisplayVersion",
        &mut info.warnings,
    )
    .or_else(|| {
        lookup_string_field(
            &hive,
            hive_path,
            "registry.software",
            &key,
            "ReleaseId",
            &mut info.warnings,
        )
    });
    info.registered_owner = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "RegisteredOwner",
        &mut info.warnings,
    );
    info.registered_organization = lookup_optional_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "RegisteredOrganization",
        &mut info.warnings,
    );
    info.product_id = lookup_string_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        "ProductId",
        &mut info.warnings,
    );
    info.install_date = lookup_install_date_field(
        &hive,
        hive_path,
        "registry.software",
        &key,
        &mut info.warnings,
    );

    Ok(info)
}

/// Like [`extract_software_hive_fields`], but after standard extraction checks a
/// transaction log for more recent writes.
pub fn extract_software_hive_fields_with_txlog(
    bytes: &[u8],
    hive_path: &str,
    txlog_data: &[u8],
) -> Result<SoftwareHiveInfo, String> {
    let mut info = extract_software_hive_fields(bytes, hive_path)?;
    let txlog = parse_transaction_log(txlog_data)?;
    let mut txlog_applied = false;
    let mut ts_infos: Vec<TxlogTimestampInfo> = Vec::new();

    let fields: [&mut Option<ParsedRegistryField>; 8] = [
        &mut info.product_name,
        &mut info.current_build,
        &mut info.current_version,
        &mut info.display_version,
        &mut info.install_date,
        &mut info.registered_owner,
        &mut info.registered_organization,
        &mut info.product_id,
    ];
    for field in fields.into_iter().flatten() {
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
    fn extract_software_fields_from_fixture() {
        let mut data = empty_hive("SOFTWARE");
        write_nk(&mut data, 0x20, "SOFTWARE", &[("Microsoft", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Microsoft", &[("Windows NT", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Windows NT",
            &[("CurrentVersion", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "CurrentVersion",
            &[],
            &[0x600, 0x680, 0x700],
        );
        write_string_value(
            &mut data,
            0x600,
            "ProductName",
            "Windows Evidence Edition",
            0x900,
        );
        write_string_value(&mut data, 0x680, "CurrentBuild", "26000", 0x980);
        write_dword_value(&mut data, 0x700, "InstallDate", 1_700_000_000);

        let info = extract_software_hive_fields(&data, "Windows/System32/config/SOFTWARE").unwrap();

        assert_eq!(info.product_name.unwrap().value, "Windows Evidence Edition");
        assert_eq!(info.current_build.unwrap().value, "26000");
        assert!(info.install_date.unwrap().value.starts_with("2023-"));
    }

    #[test]
    fn extract_software_fields_from_committed_tiny_fixture() {
        let bytes = std::fs::read(fixtures::tiny_registry_software_hive())
            .expect("read tiny SOFTWARE registry fixture");

        let info =
            extract_software_hive_fields(&bytes, "Windows/System32/config/SOFTWARE").unwrap();

        assert_eq!(
            info.product_name.as_ref().map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_PRODUCT_NAME)
        );
        assert_eq!(
            info.current_build
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_CURRENT_BUILD)
        );
        assert_eq!(
            info.display_version
                .as_ref()
                .map(|field| field.value.as_str()),
            Some(registry_fixture::SOFTWARE_DISPLAY_VERSION)
        );
        assert!(info
            .install_date
            .as_ref()
            .is_some_and(|field| field.value.starts_with("2023-")));
    }

    // ── Txlog-override tests ───────────────────────────────────────────────

    use crate::registry::txlog::fixture::{build_synthetic_log1, SyntheticEntry};

    #[test]
    fn software_hive_with_txlog_overrides_product_name() {
        // Build a SOFTWARE hive with ProductName = "Windows Old".
        let mut data = empty_hive("SOFTWARE");
        write_nk(&mut data, 0x20, "SOFTWARE", &[("Microsoft", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Microsoft", &[("Windows NT", 0x300)], &[]);
        write_nk(
            &mut data,
            0x300,
            "Windows NT",
            &[("CurrentVersion", 0x400)],
            &[],
        );
        write_nk(&mut data, 0x400, "CurrentVersion", &[], &[0x600, 0x680]);
        write_string_value(&mut data, 0x600, "ProductName", "Windows Old", 0x900);
        write_string_value(&mut data, 0x680, "CurrentBuild", "22000", 0x980);

        let txlog_bytes = build_synthetic_log1(&[SyntheticEntry {
            operation: 2, // SetValue
            sequence_number: 50,
            timestamp: Some(0x01DB_A000_0000_0000),
            key_path: "\\Registry\\Machine\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"
                .to_string(),
            value_name: Some("ProductName".to_string()),
            data_before: Some(encode_utf16le("Windows Old")),
            data_after: Some(encode_utf16le("Windows New")),
        }]);

        let info = extract_software_hive_fields_with_txlog(
            &data,
            "Windows/System32/config/SOFTWARE",
            &txlog_bytes,
        )
        .unwrap();

        assert_eq!(info.product_name.as_ref().unwrap().value, "Windows New");
        assert_eq!(
            info.current_build.as_ref().unwrap().value,
            "22000",
            "CurrentBuild should be untouched"
        );
        assert!(info.txlog_applied);
        assert_eq!(info.txlog_timestamps.len(), 2); // ProductName + CurrentBuild
        let pn_ts = info
            .txlog_timestamps
            .iter()
            .find(|ts| ts.field_name == "ProductName")
            .unwrap();
        assert!(pn_ts.txlog_used);
        let cb_ts = info
            .txlog_timestamps
            .iter()
            .find(|ts| ts.field_name == "CurrentBuild")
            .unwrap();
        assert!(!cb_ts.txlog_used);
    }
}
