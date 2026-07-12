use super::*;
use crate::registry::tests::*;
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

use crate::registry::tests::txlog_fixture::{build_synthetic_log1, SyntheticEntry};

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
        key_path: "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName"
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

// ── Service extraction tests ───────────────────────────────────────────────

fn services_hive() -> Vec<u8> {
    let mut data = empty_hive("SYSTEM");
    write_nk(
        &mut data,
        0x20,
        "SYSTEM",
        &[("Select", 0x200), ("ControlSet001", 0x300)],
        &[],
    );
    write_nk(&mut data, 0x200, "Select", &[], &[0x0e00]);
    write_dword_value(&mut data, 0x0e00, "Current", 1);
    write_nk(
        &mut data,
        0x300,
        "ControlSet001",
        &[("Services", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "Services",
        &[("TestSvc", 0x700), ("SvchostSvc", 0x900), ("DrvSvc", 0xa00)],
        &[],
    );

    // Value cells are placed in the 0x1000-0x1fff range (abs 0x2000-0x2fff),
    // below the subkey-list region that starts at abs 0x3000.
    // String data is placed at 0x3000+ (abs 0x4000+), safely above subkey lists.

    // TestSvc: own-process, delayed auto-start, fully populated values.
    write_nk(
        &mut data,
        0x700,
        "TestSvc",
        &[],
        &[
            0x1000, 0x1100, 0x1200, 0x1300, 0x1400, 0x1500, 0x1600, 0x1700,
        ],
    );
    write_dword_value(&mut data, 0x1000, "Type", 0x10);
    write_dword_value(&mut data, 0x1100, "Start", 2);
    write_dword_value(&mut data, 0x1200, "ErrorControl", 1);
    write_dword_value(&mut data, 0x1300, "DelayedAutoStart", 1);
    write_string_value(
        &mut data,
        0x1400,
        "ImagePath",
        "C:\\Windows\\svc.exe",
        0x3000,
    );
    write_string_value(&mut data, 0x1500, "DisplayName", "Test Service", 0x3100);
    write_string_value(&mut data, 0x1600, "Group", "Network", 0x3200);
    write_string_value(&mut data, 0x1700, "ObjectName", "LocalSystem", 0x3300);

    // SvchostSvc: share-process with Parameters\ServiceDll.
    write_nk(
        &mut data,
        0x900,
        "SvchostSvc",
        &[("Parameters", 0xb00)],
        &[0x1800, 0x1900, 0x1a00],
    );
    write_dword_value(&mut data, 0x1800, "Type", 0x20);
    write_dword_value(&mut data, 0x1900, "Start", 2);
    write_typed_string_value(
        &mut data,
        0x1a00,
        "ImagePath",
        REG_EXPAND_SZ,
        "%SystemRoot%\\system32\\svchost.exe -k netsvcs",
        0x3400,
    );
    write_nk(&mut data, 0xb00, "Parameters", &[], &[0x1b00]);
    write_string_value(
        &mut data,
        0x1b00,
        "ServiceDll",
        "C:\\Windows\\System32\\wuauserv.dll",
        0x3500,
    );

    // DrvSvc: kernel driver, boot start.
    write_nk(&mut data, 0xa00, "DrvSvc", &[], &[0x1c00, 0x1d00]);
    write_dword_value(&mut data, 0x1c00, "Type", 1);
    write_dword_value(&mut data, 0x1d00, "Start", 0);

    data
}

#[test]
fn extract_services_maps_type_and_start() {
    let data = services_hive();
    let info = extract_services_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    let test = info
        .services
        .iter()
        .find(|s| s.service_name == "TestSvc")
        .expect("TestSvc present");
    assert_eq!(test.service_type, ServiceType::Win32OwnProcess);
    assert_eq!(test.start_type, ServiceStartType::AutomaticDelayed);
    assert!(test.delayed_auto_start);
    assert_eq!(test.error_control, Some(1));
    assert_eq!(test.image_path.as_deref(), Some("C:\\Windows\\svc.exe"));
    assert_eq!(test.display_name.as_deref(), Some("Test Service"));
    assert_eq!(test.group.as_deref(), Some("Network"));
    assert_eq!(test.object_name.as_deref(), Some("LocalSystem"));
}

#[test]
fn extract_services_resolves_svchost_service_dll() {
    let data = services_hive();
    let info = extract_services_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    let svc = info
        .services
        .iter()
        .find(|s| s.service_name == "SvchostSvc")
        .expect("SvchostSvc present");
    assert_eq!(svc.service_type, ServiceType::Win32ShareProcess);
    assert_eq!(svc.start_type, ServiceStartType::Automatic);
    assert_eq!(
        svc.image_path.as_deref(),
        Some("%SystemRoot%\\system32\\svchost.exe -k netsvcs")
    );
    assert_eq!(
        svc.service_dll.as_deref(),
        Some("C:\\Windows\\System32\\wuauserv.dll")
    );
}

#[test]
fn extract_services_maps_kernel_driver() {
    let data = services_hive();
    let info = extract_services_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    let drv = info
        .services
        .iter()
        .find(|s| s.service_name == "DrvSvc")
        .expect("DrvSvc present");
    assert_eq!(drv.service_type, ServiceType::KernelDriver);
    assert_eq!(drv.start_type, ServiceStartType::Boot);
}

#[test]
fn extract_services_deduplicates_across_control_sets() {
    let mut data = services_hive();
    // Add a second control set with the same Services subkeys to ensure
    // each service is reported only once.
    write_nk(
        &mut data,
        0x20,
        "SYSTEM",
        &[
            ("Select", 0x200),
            ("ControlSet001", 0x300),
            ("ControlSet002", 0xc00),
        ],
        &[],
    );
    write_nk(
        &mut data,
        0xc00,
        "ControlSet002",
        &[("Services", 0xd00)],
        &[],
    );
    write_nk(
        &mut data,
        0xd00,
        "Services",
        &[("TestSvc", 0xe00), ("ExtraSvc", 0xf00)],
        &[],
    );
    write_nk(&mut data, 0xe00, "TestSvc", &[], &[0x6000, 0x6100]);
    write_dword_value(&mut data, 0x6000, "Type", 0x10);
    write_dword_value(&mut data, 0x6100, "Start", 3);
    write_nk(&mut data, 0xf00, "ExtraSvc", &[], &[0x6200]);
    write_dword_value(&mut data, 0x6200, "Type", 0x10);

    let info = extract_services_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();
    let test_count = info
        .services
        .iter()
        .filter(|s| s.service_name == "TestSvc")
        .count();
    assert_eq!(test_count, 1);
    assert!(info.services.iter().any(|s| s.service_name == "ExtraSvc"));
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
            key_path:
                "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName"
                    .to_string(),
            value_name: Some("ComputerName".to_string()),
            data_before: Some(encode_utf16le("V1")),
            data_after: Some(encode_utf16le("V2")),
        },
        SyntheticEntry {
            operation: 2,
            sequence_number: 20, // higher seq → should win
            timestamp: Some(0x01DB_A000_0000_0000),
            key_path:
                "\\Registry\\Machine\\SYSTEM\\ControlSet001\\Control\\ComputerName\\ComputerName"
                    .to_string(),
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

// ── LSA packages extraction tests ──────────────────────────────────────────

#[test]
fn extract_lsa_packages_from_fixture() {
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
    write_nk(&mut data, 0x400, "Control", &[("Lsa", 0x500)], &[]);
    write_nk(&mut data, 0x500, "Lsa", &[], &[0x1300, 0x1380, 0x1400]);
    write_multi_string_value(
        &mut data,
        0x1300,
        "Authentication Packages",
        &["msv1_0.dll", " Kerberos.dll"],
        0x4000,
    );
    write_multi_string_value(
        &mut data,
        0x1380,
        "Notification Packages",
        &["scecli.dll"],
        0x4100,
    );
    write_multi_string_value(
        &mut data,
        0x1400,
        "Security Packages",
        &["negotiate.dll", "secur32.dll"],
        0x4200,
    );

    let packages =
        extract_lsa_packages_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    assert_eq!(packages.len(), 1);
    let pkg = &packages[0];
    assert_eq!(pkg.control_set, "ControlSet001");
    assert_eq!(
        pkg.authentication_packages,
        vec!["msv1_0.dll", " Kerberos.dll"]
    );
    assert_eq!(pkg.notification_packages, vec!["scecli.dll"]);
    assert_eq!(pkg.security_packages, vec!["negotiate.dll", "secur32.dll"]);
}

// ── USB / MountedDevices extraction tests ────────────────────────────────────

fn usbstor_hive() -> Vec<u8> {
    let mut data = empty_hive("SYSTEM");
    write_nk(&mut data, 0x20, "SYSTEM", &[("Enum", 0x200)], &[]);
    write_nk(&mut data, 0x200, "Enum", &[("USBSTOR", 0x300)], &[]);
    write_nk(
        &mut data,
        0x300,
        "USBSTOR",
        &[("Disk&Ven_Kingston&Prod_DT101&Rev_1.00", 0x400)],
        &[],
    );
    write_nk(
        &mut data,
        0x400,
        "Disk&Ven_Kingston&Prod_DT101&Rev_1.00",
        &[("A1B2C3D4E5F6&0", 0x500)],
        &[],
    );
    write_nk(&mut data, 0x500, "A1B2C3D4E5F6&0", &[], &[0x1000]);
    write_string_value(
        &mut data,
        0x1000,
        "FriendlyName",
        "Kingston DT101 USB Device",
        0x3000,
    );

    // Non-zero FILETIMEs for class (first connect) and serial (last connect).
    set_nk_last_write(&mut data, 0x400, 0x01DB_9F8C_0000_0000);
    set_nk_last_write(&mut data, 0x500, 0x01DB_A000_0000_0000);

    data
}

fn set_nk_last_write(data: &mut [u8], offset: u32, filetime: u64) {
    let abs = BASE_BLOCK_SIZE + offset as usize;
    data[abs + 0x08..abs + 0x10].copy_from_slice(&filetime.to_le_bytes());
}

#[test]
fn extract_usb_devices_from_system_hive_parses_class_and_serial() {
    let data = usbstor_hive();
    let entries =
        extract_usb_devices_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.device_name, "Kingston DT101 USB Device");
    assert_eq!(entry.serial_number, "A1B2C3D4E5F6");
    assert_eq!(entry.raw_serial_number, "A1B2C3D4E5F6&0");
    assert_eq!(entry.vendor.as_deref(), Some("Kingston"));
    assert_eq!(entry.product.as_deref(), Some("DT101"));
    assert_eq!(entry.revision.as_deref(), Some("1.00"));
    assert!(entry
        .first_connect
        .as_deref()
        .unwrap()
        .starts_with("2025-03-28"));
    assert!(entry
        .last_connect
        .as_deref()
        .unwrap()
        .starts_with("2025-03-28"));
}

#[test]
fn extract_mounted_devices_from_system_hive_parses_dos_and_volume() {
    let mut data = empty_hive("SYSTEM");
    write_nk(&mut data, 0x20, "SYSTEM", &[("MountedDevices", 0x200)], &[]);
    write_nk(&mut data, 0x200, "MountedDevices", &[], &[0x1000, 0x1100]);
    write_binary_value(
        &mut data,
        0x1000,
        r"\DosDevices\C:",
        &[0xDE, 0xAD, 0xBE, 0xEF],
        0x3000,
    );
    write_binary_value(
        &mut data,
        0x1100,
        r"\\?\Volume{12345678-1234-1234-1234-123456789abc}",
        &[0xCA, 0xFE, 0xBA, 0xBE],
        0x3100,
    );

    let entries =
        extract_mounted_devices_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    assert_eq!(entries.len(), 2);
    let dos = entries
        .iter()
        .find(|e| e.device_name == r"\DosDevices\C:")
        .expect("DOS device entry");
    assert_eq!(dos.drive_letter.as_deref(), Some("C:"));
    assert_eq!(dos.volume_guid.as_ref(), None);
    assert_eq!(dos.disk_signature_hex.as_deref(), Some("deadbeef"));

    let vol = entries
        .iter()
        .find(|e| e.device_name.starts_with(r"\\?\Volume{"))
        .expect("volume entry");
    assert_eq!(vol.drive_letter.as_ref(), None);
    assert_eq!(
        vol.volume_guid.as_deref(),
        Some("12345678-1234-1234-1234-123456789abc")
    );
    assert_eq!(vol.disk_signature_hex.as_deref(), Some("cafebabe"));
}

// ── ShutdownTime / ShimCache extraction tests ────────────────────────────────

fn make_shutdown_time_hive(filetime: u64) -> Vec<u8> {
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
    write_nk(&mut data, 0x400, "Control", &[("Windows", 0x500)], &[]);
    write_nk(&mut data, 0x500, "Windows", &[], &[0x1300]);
    write_binary_value(
        &mut data,
        0x1300,
        "ShutdownTime",
        &filetime.to_le_bytes(),
        0x4000,
    );
    data
}

#[test]
fn extract_shutdown_time_from_fixture() {
    let filetime = 0x01DB_A000_0000_0000u64;
    let data = make_shutdown_time_hive(filetime);

    let entries =
        extract_shutdown_time_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].key_path, "ControlSet001\\Control\\Windows");
    assert!(entries[0].shutdown_time.starts_with("2025-03-28"));
}

#[test]
fn extract_shutdown_time_accepts_qword_value() {
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
    write_nk(&mut data, 0x400, "Control", &[("Windows", 0x500)], &[]);
    write_nk(&mut data, 0x500, "Windows", &[], &[0x1300]);
    let filetime = 0x01DB_A000_0000_0000u64;
    write_qword_value(&mut data, 0x1300, "ShutdownTime", filetime, 0x4000);

    let entries =
        extract_shutdown_time_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    assert_eq!(entries.len(), 1);
    assert!(entries[0].shutdown_time.starts_with("2025-03-28"));
}

fn make_win10_shimcache_blob(path: &str) -> Vec<u8> {
    let mut header = vec![0u8; 0x30];
    header[0..4].copy_from_slice(&0x30u32.to_le_bytes());
    let path_utf16: Vec<u8> = path.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let path_len = path_utf16.len();
    let data_len = 4u16;
    let entry_len = 26 + path_len + data_len as usize;
    let mut entry = Vec::with_capacity(entry_len);
    entry.extend_from_slice(b"10ts");
    entry.extend_from_slice(&0u32.to_le_bytes()); // unknown
    entry.extend_from_slice(&(entry_len as u32).to_le_bytes());
    entry.extend_from_slice(&(path_len as u16).to_le_bytes());
    entry.extend_from_slice(&path_utf16);
    let filetime = 0x01DB_9F8C_0000_0000u64;
    entry.extend_from_slice(&filetime.to_le_bytes());
    entry.extend_from_slice(&data_len.to_le_bytes());
    entry.extend_from_slice(&0u16.to_le_bytes()); // data values (2 bytes when data_len == 4)
    entry.extend_from_slice(&0u16.to_le_bytes()); // execution flag
    entry.extend_from_slice(&0u16.to_le_bytes()); // padding
    header.extend(entry);
    header
}

fn make_shimcache_hive(blob: &[u8]) -> Vec<u8> {
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
        &[("Session Manager", 0x500)],
        &[],
    );
    write_nk(
        &mut data,
        0x500,
        "Session Manager",
        &[("AppCompatCache", 0x600)],
        &[],
    );
    write_nk(&mut data, 0x600, "AppCompatCache", &[], &[0x1300]);
    write_binary_value(&mut data, 0x1300, "AppCompatCache", blob, 0x4000);
    data
}

#[test]
fn extract_shimcache_from_fixture() {
    // Keep the path short so the whole synthetic blob fits into the
    // default 128-byte binary data cell used by write_binary_value.
    let blob = make_win10_shimcache_blob(r"C:\Windows\cmd.exe");
    let data = make_shimcache_hive(&blob);

    let entries =
        extract_shimcache_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, r"C:\Windows\cmd.exe");
    assert!(entries[0]
        .last_modified
        .as_deref()
        .unwrap()
        .starts_with("2025-03-28"));
    assert_eq!(
        entries[0].source_key_path,
        "ControlSet001\\Control\\Session Manager\\AppCompatCache"
    );
}

#[test]
fn extract_shimcache_fallback_embedded_paths() {
    // No valid header/entry stream, but the blob contains a UTF-16LE Windows path.
    let path = r"C:\Windows\explorer.exe";
    let mut blob: Vec<u8> = vec![0u8; 0x20];
    blob.extend(path.encode_utf16().flat_map(u16::to_le_bytes));
    blob.extend_from_slice(&[0x00, 0x00]);
    let data = make_shimcache_hive(&blob);

    let entries =
        extract_shimcache_from_system_hive(&data, "Windows/System32/config/SYSTEM").unwrap();

    assert!(entries.iter().any(|e| e.path == path));
}
