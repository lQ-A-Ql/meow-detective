use super::*;
use crate::registry::tests::*;

#[test]
fn extract_amcache_applications_from_fixture() {
    let mut data = empty_hive("Root");
    write_nk(
        &mut data,
        0x20,
        "Root",
        &[("InventoryApplication", 0x200)],
        &[],
    );
    write_nk(
        &mut data,
        0x200,
        "InventoryApplication",
        &[("app-0001", 0x400)],
        &[],
    );
    write_nk(
        &mut data,
        0x400,
        "app-0001",
        &[],
        &[0x1000, 0x1080, 0x1100, 0x1180],
    );
    write_string_value(&mut data, 0x1000, "Name", "Contoso App", 0x3000);
    write_string_value(&mut data, 0x1080, "Version", "1.2.3", 0x3080);
    write_string_value(&mut data, 0x1100, "Publisher", "Contoso Ltd.", 0x3100);
    write_string_value(&mut data, 0x1180, "Source", "AddRemoveProgram", 0x3180);

    let info = extract_amcache_entries(&data, "Windows/AppCompat/Programs/Amcache.hve").unwrap();

    assert_eq!(info.applications.len(), 1);
    let app = &info.applications[0];
    assert_eq!(app.name.as_deref(), Some("Contoso App"));
    assert_eq!(app.version.as_deref(), Some("1.2.3"));
    assert_eq!(app.publisher.as_deref(), Some("Contoso Ltd."));
    assert_eq!(app.source.as_deref(), Some("AddRemoveProgram"));
    assert_eq!(
        app.registry_key_path,
        "Root\\InventoryApplication\\app-0001"
    );
}

#[test]
fn extract_amcache_application_files_from_fixture() {
    let mut data = empty_hive("Root");
    write_nk(
        &mut data,
        0x20,
        "Root",
        &[("InventoryApplicationFile", 0x200)],
        &[],
    );
    write_nk(
        &mut data,
        0x200,
        "InventoryApplicationFile",
        &[("file-0001", 0x400)],
        &[],
    );
    write_nk(
        &mut data,
        0x400,
        "file-0001",
        &[],
        &[0x1000, 0x1080, 0x1100, 0x1180],
    );
    write_string_value(
        &mut data,
        0x1000,
        "LowerCaseLongPath",
        "c:\\program files\\contoso\\app.exe",
        0x3000,
    );
    write_string_value(&mut data, 0x1080, "LongPathHash", "deadbeef", 0x3080);
    write_qword_value(&mut data, 0x1100, "FileSize", 1_048_576, 0x3100);
    write_string_value(&mut data, 0x1180, "ProgramId", "prog-1234", 0x3180);

    let info = extract_amcache_entries(&data, "Windows/AppCompat/Programs/Amcache.hve").unwrap();

    assert_eq!(info.application_files.len(), 1);
    let file = &info.application_files[0];
    assert_eq!(
        file.lower_case_long_path.as_deref(),
        Some("c:\\program files\\contoso\\app.exe")
    );
    assert_eq!(file.long_path_hash.as_deref(), Some("deadbeef"));
    assert_eq!(file.file_size, Some(1_048_576));
    assert_eq!(file.program_id.as_deref(), Some("prog-1234"));
    assert_eq!(
        file.registry_key_path,
        "Root\\InventoryApplicationFile\\file-0001"
    );
}
