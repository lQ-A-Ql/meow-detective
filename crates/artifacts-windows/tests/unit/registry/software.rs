use super::*;
use crate::registry::tests::txlog_fixture::{build_synthetic_log1, SyntheticEntry};
use crate::registry::tests::*;
use testing::{builders::registry as registry_fixture, fixtures};

fn software_current_version_hive() -> Vec<u8> {
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
        &[0x600, 0x680, 0x700, 0x780],
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
    write_dword_value(&mut data, 0x780, "UBR", 26100);
    data
}

#[test]
fn extracts_software_identity_fields() {
    let info = extract_software_hive_fields(
        &software_current_version_hive(),
        "Windows/System32/config/SOFTWARE",
    )
    .unwrap();
    assert_eq!(info.product_name.unwrap().value, "Windows Evidence Edition");
    assert_eq!(info.current_build.unwrap().value, "26000");
    assert_eq!(info.update_build_revision.unwrap().value, "26100");
    assert!(info.install_date.unwrap().value.starts_with("2023-"));
}

#[test]
fn committed_software_fixture_remains_compatible() {
    let bytes =
        std::fs::read(fixtures::tiny_registry_software_hive()).expect("read SOFTWARE fixture");
    let info = extract_software_hive_fields(&bytes, "Windows/System32/config/SOFTWARE").unwrap();
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

#[test]
fn transaction_log_overrides_matching_field_only() {
    let data = software_current_version_hive();
    let key_path = "\\Registry\\Machine\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion";
    let log = build_synthetic_log1(&[
        SyntheticEntry {
            operation: 2,
            sequence_number: 50,
            timestamp: Some(0x01db_a000_0000_0000),
            key_path: key_path.to_string(),
            value_name: Some("ProductName".to_string()),
            data_before: Some(encode_utf16le("Windows Evidence Edition")),
            data_after: Some(encode_utf16le("Windows New")),
        },
        SyntheticEntry {
            operation: 2,
            sequence_number: 51,
            timestamp: Some(0x01db_a000_0000_0001),
            key_path: key_path.to_string(),
            value_name: Some("UBR".to_string()),
            data_before: Some(26100u32.to_le_bytes().to_vec()),
            data_after: Some(26101u32.to_le_bytes().to_vec()),
        },
    ]);
    let info =
        extract_software_hive_fields_with_txlog(&data, "Windows/System32/config/SOFTWARE", &log)
            .unwrap();
    assert_eq!(info.product_name.unwrap().value, "Windows New");
    assert_eq!(info.current_build.unwrap().value, "26000");
    assert_eq!(info.update_build_revision.unwrap().value, "26101");
    assert!(info.txlog_applied);
    let product_name_timestamp = info
        .txlog_timestamps
        .iter()
        .find(|timestamp| timestamp.field_name == "ProductName")
        .unwrap();
    assert!(product_name_timestamp.txlog_used);
    let current_build_timestamp = info
        .txlog_timestamps
        .iter()
        .find(|timestamp| timestamp.field_name == "CurrentBuild")
        .unwrap();
    assert!(!current_build_timestamp.txlog_used);
    let revision_timestamp = info
        .txlog_timestamps
        .iter()
        .find(|timestamp| timestamp.field_name == "UBR")
        .unwrap();
    assert!(revision_timestamp.txlog_used);
}

fn set_last_write(data: &mut [u8], offset: u32, filetime: u64) {
    let absolute = 0x1000 + offset as usize;
    data[absolute + 0x08..absolute + 0x10].copy_from_slice(&filetime.to_le_bytes());
}

#[test]
fn extracts_machine_run_and_winlogon_values() {
    let mut data = empty_hive("SOFTWARE");
    write_nk(&mut data, 0x20, "SOFTWARE", &[("Microsoft", 0x200)], &[]);
    write_nk(
        &mut data,
        0x200,
        "Microsoft",
        &[("Windows", 0x300), ("Windows NT", 0x700)],
        &[],
    );
    write_nk(
        &mut data,
        0x300,
        "Windows",
        &[("CurrentVersion", 0x400)],
        &[],
    );
    write_nk(&mut data, 0x400, "CurrentVersion", &[("Run", 0x500)], &[]);
    write_nk(&mut data, 0x500, "Run", &[], &[0x1000]);
    write_string_value(
        &mut data,
        0x1000,
        "OneDrive",
        "C:\\Program Files\\OneDrive.exe",
        0x3000,
    );
    set_last_write(&mut data, 0x500, 0x01db_a000_0000_0000);
    write_nk(
        &mut data,
        0x700,
        "Windows NT",
        &[("CurrentVersion", 0x800)],
        &[],
    );
    write_nk(
        &mut data,
        0x800,
        "CurrentVersion",
        &[("Winlogon", 0x900)],
        &[],
    );
    write_nk(&mut data, 0x900, "Winlogon", &[], &[0x1080, 0x1100]);
    write_string_value(&mut data, 0x1080, "Shell", "explorer.exe", 0x3100);
    write_string_value(
        &mut data,
        0x1100,
        "Userinit",
        "C:\\Windows\\system32\\userinit.exe,",
        0x3200,
    );

    let run_keys = extract_machine_run_keys_from_software_hive(&data, "SOFTWARE").unwrap();
    assert_eq!(run_keys.len(), 1);
    assert_eq!(run_keys[0].value_name, "OneDrive");
    assert_eq!(run_keys[0].scope, "machine");
    let winlogon = extract_winlogon_fields_from_software_hive(&data, "SOFTWARE").unwrap();
    assert_eq!(winlogon.shell.as_deref(), Some("explorer.exe"));
    assert_eq!(
        winlogon.userinit.as_deref(),
        Some("C:\\Windows\\system32\\userinit.exe,")
    );
}

#[test]
fn extracts_appcompat_layers() {
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
        &[("AppCompatFlags", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "AppCompatFlags",
        &[("Layers", 0x600)],
        &[],
    );
    write_nk(&mut data, 0x600, "Layers", &[], &[0x1000]);
    write_string_value(
        &mut data,
        0x1000,
        "C:\\Windows\\notepad.exe",
        "WINXPSP3 RUNASADMIN",
        0x3000,
    );
    let entries = extract_appcompat_layers_from_software_hive(&data, "SOFTWARE").unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].layer_string, "WINXPSP3 RUNASADMIN");
}

#[test]
fn extract_network_profiles_from_fixture() {
    let guid = "{12345678-1234-1234-1234-123456789abc}";
    let system_time = |year: u16,
                       month: u16,
                       day_of_week: u16,
                       day: u16,
                       hour: u16,
                       minute: u16,
                       second: u16,
                       millis: u16| {
        let mut bytes = Vec::with_capacity(16);
        for value in [year, month, day_of_week, day, hour, minute, second, millis] {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes
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
        &system_time(2024, 8, 1, 12, 8, 0, 0, 0),
        0x3100,
    );
    write_binary_value(
        &mut data,
        0x1100,
        "DateLastConnected",
        &system_time(2025, 3, 6, 16, 18, 30, 0, 0),
        0x3200,
    );
    write_dword_value(&mut data, 0x1180, "NameType", 71);
    write_dword_value(&mut data, 0x1200, "Managed", 0);
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
    assert!(profile
        .date_created
        .as_ref()
        .is_some_and(|value| value.starts_with("2024-08-12T08:00:00")));
    assert!(profile
        .date_last_connected
        .as_ref()
        .is_some_and(|value| value.starts_with("2025-03-16T18:30:00")));
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
fn preserves_detailed_software_projection_metadata() {
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
    set_last_write(&mut data, 0x500, 0x01db_a000_0000_0000);
    write_nk(&mut data, 0x580, "RunOnce", &[], &[]);
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
        &[("Winlogon", 0xd00), ("AppCompatFlags", 0xe00)],
        &[],
    );
    write_nk(
        &mut data,
        0xd00,
        "Winlogon",
        &[],
        &[0x1180, 0x1200, 0x1280, 0x1300, 0x1380, 0x1400],
    );
    write_string_value(&mut data, 0x1180, "Shell", "explorer.exe", 0x3300);
    write_string_value(
        &mut data,
        0x1200,
        "Userinit",
        "C:\\Windows\\system32\\userinit.exe,",
        0x3400,
    );
    write_string_value(&mut data, 0x1280, "Notify", "sclgntfy.dll", 0x3500);
    write_string_value(&mut data, 0x1300, "AutoAdminLogon", "0", 0x3600);
    write_string_value(&mut data, 0x1380, "DefaultDomainName", "CORP", 0x3700);
    write_string_value(&mut data, 0x1400, "DefaultUserName", "Admin", 0x3800);
    write_nk(
        &mut data,
        0xe00,
        "AppCompatFlags",
        &[("Layers", 0xf00)],
        &[],
    );
    write_nk(&mut data, 0xf00, "Layers", &[], &[0x1480, 0x1500]);
    write_string_value(
        &mut data,
        0x1480,
        "C:\\Windows\\System32\\notepad.exe",
        "WINXPSP3 RUNASADMIN",
        0x3900,
    );
    write_string_value(
        &mut data,
        0x1500,
        "C:\\Program Files\\App\\app.exe",
        "ELEVATECREATEPROCESS",
        0x3a00,
    );
    set_last_write(&mut data, 0xf00, 0x01db_a000_0000_0000);

    let run_keys =
        extract_machine_run_keys_from_software_hive(&data, "Windows/System32/config/SOFTWARE")
            .unwrap();
    assert_eq!(run_keys.len(), 3);
    let one_drive = run_keys
        .iter()
        .find(|entry| entry.value_name == "OneDrive")
        .unwrap();
    assert_eq!(
        one_drive.key_path,
        "Microsoft\\Windows\\CurrentVersion\\Run"
    );
    assert!(one_drive.command.contains("OneDrive.exe"));
    assert_eq!(one_drive.scope, "machine");
    assert!(one_drive
        .timestamp
        .as_ref()
        .is_some_and(|value| value.starts_with("2025-03-28")));
    let setup = run_keys
        .iter()
        .find(|entry| entry.value_name == "Setup")
        .unwrap();
    assert_eq!(
        setup.key_path,
        "WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\RunOnce"
    );

    let winlogon =
        extract_winlogon_fields_from_software_hive(&data, "Windows/System32/config/SOFTWARE")
            .unwrap();
    assert_eq!(
        winlogon.key_path,
        "Microsoft\\Windows NT\\CurrentVersion\\Winlogon"
    );
    assert_eq!(winlogon.notify.as_deref(), Some("sclgntfy.dll"));
    assert_eq!(winlogon.auto_admin_logon.as_deref(), Some("0"));
    assert_eq!(winlogon.default_domain_name.as_deref(), Some("CORP"));
    assert_eq!(winlogon.default_user_name.as_deref(), Some("Admin"));

    let layers =
        extract_appcompat_layers_from_software_hive(&data, "Windows/System32/config/SOFTWARE")
            .unwrap();
    assert_eq!(layers.len(), 2);
    let notepad = layers
        .iter()
        .find(|entry| entry.executable_path.contains("notepad.exe"))
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
        .is_some_and(|value| value.starts_with("2025-03-28")));
}
