use super::reader::RegistryHiveReader;
use super::txlog_util::apply_single_txlog_override;
use super::*;
use crate::registry::RegistryError;

// ── SOFTWARE hive field extraction ────────────────────────────────────────────

pub fn extract_software_hive_fields(
    bytes: &[u8],
    hive_path: &str,
) -> Result<SoftwareHiveInfo, RegistryError> {
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
    info.current_version = lookup_optional_string_field(
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
) -> Result<SoftwareHiveInfo, RegistryError> {
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

// ── Installed software enumeration ───────────────────────────────────────────

/// Extract installed software entries from the SOFTWARE hive Uninstall keys.
///
/// Reads both the 64-bit (`Microsoft\Windows\CurrentVersion\Uninstall`) and
/// 32-bit (`WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall`) keys.
pub fn extract_installed_software(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<InstalledSoftwareInfo>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut result = Vec::new();

    let roots: &[&[&str]] = &[
        &["Microsoft", "Windows", "CurrentVersion", "Uninstall"],
        &[
            "WOW6432Node",
            "Microsoft",
            "Windows",
            "CurrentVersion",
            "Uninstall",
        ],
    ];

    for root in roots {
        let nk = match hive.navigate_to(root) {
            Ok(Some(nk)) => nk,
            _ => continue,
        };
        let subkey_names = match hive.read_subkey_names_from_nk(&nk) {
            Ok(names) => names,
            _ => continue,
        };

        for subkey_name in subkey_names {
            let mut key_path: Vec<&str> = root.to_vec();
            key_path.push(subkey_name.as_str());

            let display_name = match read_optional_string_value(&hive, &key_path, "DisplayName") {
                Some(name) if !name.trim().is_empty() => name,
                _ => continue,
            };

            let publisher = read_optional_string_value(&hive, &key_path, "Publisher");
            let version = read_optional_string_value(&hive, &key_path, "DisplayVersion");
            let install_date = read_optional_string_value(&hive, &key_path, "InstallDate");
            let estimated_size_kb =
                read_optional_dword_value(&hive, &key_path, "EstimatedSize").map(u64::from);
            let uninstall_string = read_optional_string_value(&hive, &key_path, "UninstallString");

            result.push(InstalledSoftwareInfo {
                display_name,
                version,
                publisher,
                install_date,
                estimated_size_kb,
                uninstall_string,
                source_key: key_path.join("\\"),
            });
        }
    }

    Ok(result)
}

/// Extract machine-scope Run / RunOnce / RunOnceEx entries from the SOFTWARE hive.
///
/// Reads both the native and WOW6432Node variants and tags each entry with
/// `scope = "machine"` and the parent key's last-write FILETIME timestamp.
pub fn extract_machine_run_keys_from_software_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<RegistryRunKey>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut result = Vec::new();

    let roots: &[&[&str]] = &[
        &["Microsoft", "Windows", "CurrentVersion", "Run"],
        &["Microsoft", "Windows", "CurrentVersion", "RunOnce"],
        &["Microsoft", "Windows", "CurrentVersion", "RunOnceEx"],
        &[
            "WOW6432Node",
            "Microsoft",
            "Windows",
            "CurrentVersion",
            "Run",
        ],
        &[
            "WOW6432Node",
            "Microsoft",
            "Windows",
            "CurrentVersion",
            "RunOnce",
        ],
    ];

    for root in roots {
        let nk = match hive.navigate_to(root) {
            Ok(Some(nk)) => nk,
            _ => continue,
        };
        let timestamp = nk
            .last_write_time
            .and_then(super::windows_filetime_to_rfc3339);
        let key_path_str = root.join("\\");

        let values = match hive.read_all_values_from_nk(&nk) {
            Ok(values) => values,
            _ => continue,
        };

        for (name, value) in values {
            if let RegistryValue::String(command) = value {
                if !command.trim().is_empty() {
                    result.push(RegistryRunKey {
                        key_path: key_path_str.clone(),
                        value_name: name,
                        command,
                        timestamp: timestamp.clone(),
                        scope: "machine".to_string(),
                    });
                }
            }
        }
    }

    Ok(result)
}

/// Extract Winlogon fields from the SOFTWARE hive.
///
/// Reads `Microsoft\Windows NT\CurrentVersion\Winlogon` and returns the
/// Shell, Userinit, Notify, AutoAdminLogon, DefaultDomainName and
/// DefaultUserName values when present.
pub fn extract_winlogon_fields_from_software_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<WinlogonConfig, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key_path = ["Microsoft", "Windows NT", "CurrentVersion", "Winlogon"];
    let key_path_str = key_path.join("\\");

    Ok(WinlogonConfig {
        shell: read_optional_string_value(&hive, &key_path, "Shell"),
        userinit: read_optional_string_value(&hive, &key_path, "Userinit"),
        notify: read_optional_string_value(&hive, &key_path, "Notify"),
        auto_admin_logon: read_optional_string_value(&hive, &key_path, "AutoAdminLogon"),
        default_domain_name: read_optional_string_value(&hive, &key_path, "DefaultDomainName"),
        default_user_name: read_optional_string_value(&hive, &key_path, "DefaultUserName"),
        key_path: key_path_str,
    })
}

fn read_optional_string_value(
    hive: &RegistryHiveReader<'_>,
    key_path: &[&str],
    value_name: &str,
) -> Option<String> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::String(value))) if !value.trim().is_empty() => Some(value),
        _ => None,
    }
}

fn read_optional_dword_value(
    hive: &RegistryHiveReader<'_>,
    key_path: &[&str],
    value_name: &str,
) -> Option<u32> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::Dword(value))) => Some(value),
        _ => None,
    }
}

fn read_optional_binary_value(
    hive: &RegistryHiveReader<'_>,
    key_path: &[&str],
    value_name: &str,
) -> Option<Vec<u8>> {
    match hive.lookup_value(key_path, value_name) {
        Ok(Some(RegistryValue::Binary(value))) if !value.is_empty() => Some(value),
        _ => None,
    }
}

/// Convert a 16-byte Windows SYSTEMTIME blob to an RFC 3339 UTC timestamp.
/// The byte layout is: year, month, day_of_week, day, hour, minute, second,
/// milliseconds (each little-endian u16).
fn systemtime_bytes_to_rfc3339(data: &[u8]) -> Option<String> {
    if data.len() != 16 {
        return None;
    }
    let read_u16 = |offset: usize| u16::from_le_bytes([data[offset], data[offset + 1]]);
    let year = read_u16(0);
    let month = read_u16(2);
    let _day_of_week = read_u16(4);
    let day = read_u16(6);
    let hour = read_u16(8);
    let minute = read_u16(10);
    let second = read_u16(12);
    let millis = read_u16(14);

    let date = chrono::NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32)?;
    let time = chrono::NaiveTime::from_hms_milli_opt(
        hour as u32,
        minute as u32,
        second as u32,
        millis as u32,
    )?;
    Some(
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            chrono::NaiveDateTime::new(date, time),
            chrono::Utc,
        )
        .to_rfc3339(),
    )
}

/// Extract Windows network profile metadata from the SOFTWARE hive.
///
/// Reads `Microsoft\Windows NT\CurrentVersion\NetworkList\Profiles` and
/// merges signature data from `NetworkList\Signatures\Managed` and
/// `NetworkList\Signatures\Unmanaged`.
pub fn extract_network_profiles_from_software_hive(
    bytes: &[u8],
    _hive_path: &str,
) -> Result<Vec<NetworkProfileEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let mut profiles: std::collections::HashMap<String, NetworkProfileEntry> =
        std::collections::HashMap::new();

    let profile_root = [
        "Microsoft",
        "Windows NT",
        "CurrentVersion",
        "NetworkList",
        "Profiles",
    ];

    if let Ok(Some(nk)) = hive.navigate_to(&profile_root) {
        let subkey_names = hive.read_subkey_names_from_nk(&nk).unwrap_or_default();
        for guid in subkey_names {
            let mut key_path: Vec<&str> = profile_root.to_vec();
            key_path.push(&guid);
            let source_key_path = key_path.join("\\");

            let profile_name =
                read_optional_string_value(&hive, &key_path, "ProfileName").unwrap_or_default();
            let description = read_optional_string_value(&hive, &key_path, "Description");
            let date_created = read_optional_binary_value(&hive, &key_path, "DateCreated")
                .and_then(|b| systemtime_bytes_to_rfc3339(&b));
            let date_last_connected =
                read_optional_binary_value(&hive, &key_path, "DateLastConnected")
                    .and_then(|b| systemtime_bytes_to_rfc3339(&b));
            let name_type = read_optional_dword_value(&hive, &key_path, "NameType");
            let managed = read_optional_dword_value(&hive, &key_path, "Managed")
                .map(|v| v != 0)
                .unwrap_or(false);

            profiles.insert(
                guid.clone(),
                NetworkProfileEntry {
                    profile_guid: guid,
                    profile_name,
                    description,
                    date_created,
                    date_last_connected,
                    name_type,
                    managed,
                    first_network: None,
                    default_gateway_mac_hex: None,
                    dns_suffix: None,
                    source_key_path,
                },
            );
        }
    }

    for (signature_root, managed) in [
        (
            [
                "Microsoft",
                "Windows NT",
                "CurrentVersion",
                "NetworkList",
                "Signatures",
                "Managed",
            ],
            true,
        ),
        (
            [
                "Microsoft",
                "Windows NT",
                "CurrentVersion",
                "NetworkList",
                "Signatures",
                "Unmanaged",
            ],
            false,
        ),
    ] {
        if let Ok(Some(nk)) = hive.navigate_to(&signature_root) {
            let subkey_names = hive.read_subkey_names_from_nk(&nk).unwrap_or_default();
            for guid in subkey_names {
                let mut key_path: Vec<&str> = signature_root.to_vec();
                key_path.push(&guid);

                let profile_guid = read_optional_string_value(&hive, &key_path, "ProfileGuid")
                    .unwrap_or_else(|| {
                        // Fallback to signature subkey name when ProfileGuid is absent.
                        guid.clone()
                    });

                if let Some(entry) = profiles.get_mut(&profile_guid) {
                    entry.managed = managed;
                    entry.first_network =
                        read_optional_string_value(&hive, &key_path, "FirstNetwork");
                    entry.default_gateway_mac_hex =
                        read_optional_binary_value(&hive, &key_path, "DefaultGatewayMac")
                            .map(hex::encode);
                    entry.dns_suffix = read_optional_string_value(&hive, &key_path, "DnsSuffix");
                } else {
                    // Signature exists without a matching Profiles entry; still
                    // report it using the signature data we have.
                    let source_key_path = key_path.join("\\");
                    profiles.insert(
                        profile_guid.clone(),
                        NetworkProfileEntry {
                            profile_guid: profile_guid.clone(),
                            profile_name: String::new(),
                            description: None,
                            date_created: None,
                            date_last_connected: None,
                            name_type: None,
                            managed,
                            first_network: read_optional_string_value(
                                &hive,
                                &key_path,
                                "FirstNetwork",
                            ),
                            default_gateway_mac_hex: read_optional_binary_value(
                                &hive,
                                &key_path,
                                "DefaultGatewayMac",
                            )
                            .map(hex::encode),
                            dns_suffix: read_optional_string_value(&hive, &key_path, "DnsSuffix"),
                            source_key_path,
                        },
                    );
                }
            }
        }
    }

    let mut result: Vec<NetworkProfileEntry> = profiles.into_values().collect();
    result.sort_by(|a, b| a.profile_guid.cmp(&b.profile_guid));
    Ok(result)
}

/// Extract program compatibility / elevation flags from
/// `Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers` in the
/// SOFTWARE hive.
pub fn extract_appcompat_layers_from_software_hive(
    bytes: &[u8],
    hive_path: &str,
) -> Result<Vec<AppCompatLayerEntry>, RegistryError> {
    let hive = RegistryHiveReader::new(bytes)?;
    let key_path: &[&str] = &[
        "Microsoft",
        "Windows NT",
        "CurrentVersion",
        "AppCompatFlags",
        "Layers",
    ];

    let nk = match hive.navigate_to(key_path) {
        Ok(Some(nk)) => nk,
        Ok(None) => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };

    let last_write = nk
        .last_write_time
        .and_then(super::windows_filetime_to_rfc3339);
    let source_key_path = key_path.join("\\");
    let values = hive.read_all_values_from_nk(&nk)?;
    let mut entries = Vec::new();

    for (name, value) in values {
        if let RegistryValue::String(layer_string) = value {
            if !name.trim().is_empty() || !layer_string.trim().is_empty() {
                entries.push(AppCompatLayerEntry {
                    executable_path: name,
                    layer_string,
                    source_hive_path: hive_path.to_string(),
                    source_key_path: source_key_path.clone(),
                    last_write: last_write.clone(),
                });
            }
        }
    }

    Ok(entries)
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

    // ── Machine Run / Winlogon extraction tests ──────────────────────────────

    fn set_nk_last_write(data: &mut [u8], offset: u32, filetime: u64) {
        let abs = BASE_BLOCK_SIZE + offset as usize;
        data[abs + 0x08..abs + 0x10].copy_from_slice(&filetime.to_le_bytes());
    }

    #[test]
    fn extract_machine_run_keys_from_fixture() {
        // Use offsets starting at 0x1000 for leaf keys to avoid colliding with
        // the 0x200/0x300/0x400 internal nodes and their subkey/value lists.
        let mut data = empty_hive("SOFTWARE");
        write_nk(
            &mut data,
            0x20,
            "SOFTWARE",
            &[("Microsoft", 0x200), ("WOW6432Node", 0x600)],
            &[],
        );
        write_nk(
            &mut data,
            0x200,
            "Microsoft",
            &[("Windows", 0x300), ("Windows NT", 0xb00)],
            &[],
        );

        // Microsoft\Windows\CurrentVersion\Run
        write_nk(
            &mut data,
            0x300,
            "Windows",
            &[("CurrentVersion", 0x400)],
            &[],
        );
        write_nk(
            &mut data,
            0x400,
            "CurrentVersion",
            &[("Run", 0x500), ("RunOnce", 0x580)],
            &[],
        );
        write_nk(&mut data, 0x500, "Run", &[], &[0x1000, 0x1080]);
        write_string_value(
            &mut data,
            0x1000,
            "OneDrive",
            "C:\\Program Files\\Microsoft OneDrive\\OneDrive.exe /background",
            0x3000,
        );
        write_string_value(
            &mut data,
            0x1080,
            "SecurityHealth",
            "%ProgramFiles%\\Windows Defender\\MSASCuiL.exe",
            0x3080,
        );
        set_nk_last_write(&mut data, 0x500, 0x01DB_A000_0000_0000);

        // RunOnce sibling (empty) — included to show it is skipped when absent.
        write_nk(&mut data, 0x580, "RunOnce", &[], &[]);

        // WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce
        write_nk(
            &mut data,
            0x600,
            "WOW6432Node",
            &[("Microsoft", 0x700)],
            &[],
        );
        write_nk(&mut data, 0x700, "Microsoft", &[("Windows", 0x800)], &[]);
        write_nk(
            &mut data,
            0x800,
            "Windows",
            &[("CurrentVersion", 0x900)],
            &[],
        );
        write_nk(
            &mut data,
            0x900,
            "CurrentVersion",
            &[("RunOnce", 0xa00)],
            &[],
        );
        write_nk(&mut data, 0xa00, "RunOnce", &[], &[0x1100]);
        write_string_value(
            &mut data,
            0x1100,
            "Setup",
            "C:\\Windows\\Setup.exe /silent",
            0x3200,
        );

        let keys =
            extract_machine_run_keys_from_software_hive(&data, "Windows/System32/config/SOFTWARE")
                .unwrap();

        assert_eq!(keys.len(), 3);
        let onedrive = keys.iter().find(|k| k.value_name == "OneDrive").unwrap();
        assert_eq!(onedrive.key_path, "Microsoft\\Windows\\CurrentVersion\\Run");
        assert!(onedrive.command.contains("OneDrive.exe"));
        assert_eq!(onedrive.scope, "machine");
        assert!(onedrive
            .timestamp
            .as_ref()
            .unwrap()
            .starts_with("2025-03-28"));

        let setup = keys.iter().find(|k| k.value_name == "Setup").unwrap();
        assert_eq!(
            setup.key_path,
            "WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\RunOnce"
        );
        assert_eq!(setup.scope, "machine");
    }

    #[test]
    fn extract_winlogon_from_fixture() {
        let mut data = empty_hive("SOFTWARE");
        write_nk(&mut data, 0x20, "SOFTWARE", &[("Microsoft", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Microsoft", &[("Windows NT", 0xb00)], &[]);
        write_nk(
            &mut data,
            0xb00,
            "Windows NT",
            &[("CurrentVersion", 0xc00)],
            &[],
        );
        write_nk(
            &mut data,
            0xc00,
            "CurrentVersion",
            &[("Winlogon", 0xd00)],
            &[],
        );
        write_nk(
            &mut data,
            0xd00,
            "Winlogon",
            &[],
            &[0x1000, 0x1080, 0x1100, 0x1180, 0x1200, 0x1280],
        );
        write_string_value(&mut data, 0x1000, "Shell", "explorer.exe", 0x3000);
        write_string_value(
            &mut data,
            0x1080,
            "Userinit",
            "C:\\Windows\\system32\\userinit.exe,",
            0x3100,
        );
        write_string_value(&mut data, 0x1100, "Notify", "sclgntfy.dll", 0x3200);
        write_string_value(&mut data, 0x1180, "AutoAdminLogon", "0", 0x3300);
        write_string_value(&mut data, 0x1200, "DefaultDomainName", "CORP", 0x3400);
        write_string_value(&mut data, 0x1280, "DefaultUserName", "Admin", 0x3500);

        let config =
            extract_winlogon_fields_from_software_hive(&data, "Windows/System32/config/SOFTWARE")
                .unwrap();

        assert_eq!(
            config.key_path,
            "Microsoft\\Windows NT\\CurrentVersion\\Winlogon"
        );
        assert_eq!(config.shell.as_deref(), Some("explorer.exe"));
        assert_eq!(
            config.userinit.as_deref(),
            Some("C:\\Windows\\system32\\userinit.exe,")
        );
        assert_eq!(config.notify.as_deref(), Some("sclgntfy.dll"));
        assert_eq!(config.auto_admin_logon.as_deref(), Some("0"));
        assert_eq!(config.default_domain_name.as_deref(), Some("CORP"));
        assert_eq!(config.default_user_name.as_deref(), Some("Admin"));
    }

    #[test]
    fn extract_network_profiles_from_fixture() {
        let guid = "{12345678-1234-1234-1234-123456789abc}";
        let make_systemtime = |year: u16,
                               month: u16,
                               day_of_week: u16,
                               day: u16,
                               hour: u16,
                               minute: u16,
                               second: u16,
                               millis: u16| {
            let mut data = Vec::with_capacity(16);
            data.extend_from_slice(&year.to_le_bytes());
            data.extend_from_slice(&month.to_le_bytes());
            data.extend_from_slice(&day_of_week.to_le_bytes());
            data.extend_from_slice(&day.to_le_bytes());
            data.extend_from_slice(&hour.to_le_bytes());
            data.extend_from_slice(&minute.to_le_bytes());
            data.extend_from_slice(&second.to_le_bytes());
            data.extend_from_slice(&millis.to_le_bytes());
            data
        };
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
            &[("NetworkList", 0x500)],
            &[],
        );
        write_nk(
            &mut data,
            0x500,
            "NetworkList",
            &[("Profiles", 0x600), ("Signatures", 0x700)],
            &[],
        );
        write_nk(&mut data, 0x600, "Profiles", &[(guid, 0x800)], &[]);
        write_nk(&mut data, 0x700, "Signatures", &[("Unmanaged", 0x900)], &[]);
        write_nk(&mut data, 0x900, "Unmanaged", &[(guid, 0xa00)], &[]);

        // Profiles\{guid} values: ProfileName, DateCreated, DateLastConnected, NameType, Managed
        write_nk(
            &mut data,
            0x800,
            guid,
            &[],
            &[0x1000, 0x1080, 0x1100, 0x1180, 0x1200],
        );
        write_string_value(&mut data, 0x1000, "ProfileName", "Office-Corp", 0x3000);
        write_binary_value(
            &mut data,
            0x1080,
            "DateCreated",
            &make_systemtime(2024, 8, 1, 12, 8, 0, 0, 0),
            0x3100,
        );
        write_binary_value(
            &mut data,
            0x1100,
            "DateLastConnected",
            &make_systemtime(2025, 3, 6, 16, 18, 30, 0, 0),
            0x3200,
        );
        write_dword_value(&mut data, 0x1180, "NameType", 71);
        write_dword_value(&mut data, 0x1200, "Managed", 0);

        // Signatures\Unmanaged\{guid} values: FirstNetwork, DefaultGatewayMac, ProfileGuid
        write_nk(&mut data, 0xa00, guid, &[], &[0x1280, 0x1300, 0x1380]);
        write_string_value(&mut data, 0x1280, "FirstNetwork", "Office-Corp-5G", 0x3300);
        write_binary_value(
            &mut data,
            0x1300,
            "DefaultGatewayMac",
            &[0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
            0x3400,
        );
        write_string_value(&mut data, 0x1380, "ProfileGuid", guid, 0x3500);

        let profiles =
            extract_network_profiles_from_software_hive(&data, "Windows/System32/config/SOFTWARE")
                .unwrap();

        assert_eq!(profiles.len(), 1);
        let profile = &profiles[0];
        assert_eq!(profile.profile_guid, guid);
        assert_eq!(profile.profile_name, "Office-Corp");
        assert!(
            profile
                .date_created
                .as_ref()
                .unwrap()
                .starts_with("2024-08-12T08:00:00"),
            "unexpected dateCreated: {:?}",
            profile.date_created
        );
        assert!(
            profile
                .date_last_connected
                .as_ref()
                .unwrap()
                .starts_with("2025-03-16T18:30:00"),
            "unexpected dateLastConnected: {:?}",
            profile.date_last_connected
        );
        assert_eq!(profile.name_type, Some(71));
        assert!(!profile.managed);
        assert_eq!(profile.first_network.as_deref(), Some("Office-Corp-5G"));
        assert_eq!(
            profile.default_gateway_mac_hex.as_deref(),
            Some("001122334455")
        );
        assert_eq!(
            profile.source_key_path,
            format!("Microsoft\\Windows NT\\CurrentVersion\\NetworkList\\Profiles\\{guid}")
        );
    }

    #[test]
    fn extract_appcompat_layers_from_software_fixture() {
        let mut data = empty_hive("SOFTWARE");
        write_nk(&mut data, 0x20, "SOFTWARE", &[("Microsoft", 0x200)], &[]);
        write_nk(&mut data, 0x200, "Microsoft", &[("Windows NT", 0xb00)], &[]);
        write_nk(
            &mut data,
            0xb00,
            "Windows NT",
            &[("CurrentVersion", 0xc00)],
            &[],
        );
        write_nk(
            &mut data,
            0xc00,
            "CurrentVersion",
            &[("AppCompatFlags", 0xd00)],
            &[],
        );
        write_nk(
            &mut data,
            0xd00,
            "AppCompatFlags",
            &[("Layers", 0xe00)],
            &[],
        );
        write_nk(&mut data, 0xe00, "Layers", &[], &[0x1500, 0x1580]);
        write_string_value(
            &mut data,
            0x1500,
            "C:\\Windows\\System32\\notepad.exe",
            "WINXPSP3 RUNASADMIN",
            0x4000,
        );
        write_string_value(
            &mut data,
            0x1580,
            "C:\\Program Files\\App\\app.exe",
            "ELEVATECREATEPROCESS",
            0x4100,
        );
        set_nk_last_write(&mut data, 0xe00, 0x01DB_A000_0000_0000);

        let entries =
            extract_appcompat_layers_from_software_hive(&data, "Windows/System32/config/SOFTWARE")
                .unwrap();

        assert_eq!(entries.len(), 2);
        let notepad = entries
            .iter()
            .find(|e| e.executable_path.contains("notepad.exe"))
            .unwrap();
        assert_eq!(notepad.layer_string, "WINXPSP3 RUNASADMIN");
        assert_eq!(
            notepad.source_key_path,
            "Microsoft\\Windows NT\\CurrentVersion\\AppCompatFlags\\Layers"
        );
        assert_eq!(notepad.source_hive_path, "Windows/System32/config/SOFTWARE");
        assert!(notepad
            .last_write
            .as_ref()
            .unwrap()
            .starts_with("2025-03-28"));

        let app = entries
            .iter()
            .find(|e| e.executable_path.contains("app.exe"))
            .unwrap();
        assert_eq!(app.layer_string, "ELEVATECREATEPROCESS");
    }
}
