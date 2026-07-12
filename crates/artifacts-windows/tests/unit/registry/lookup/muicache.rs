use super::*;
use crate::registry::tests::*;

#[test]
fn extract_muicache_from_fixture() {
    let mut data = empty_hive("USRCLASS");
    write_nk(
        &mut data,
        0x20,
        "USRCLASS",
        &[("Local Settings", 0x200)],
        &[],
    );
    write_nk(
        &mut data,
        0x200,
        "Local Settings",
        &[("Software", 0x300)],
        &[],
    );
    write_nk(&mut data, 0x300, "Software", &[("Microsoft", 0x400)], &[]);
    write_nk(&mut data, 0x400, "Microsoft", &[("Windows", 0x500)], &[]);
    write_nk(&mut data, 0x500, "Windows", &[("Shell", 0x600)], &[]);
    write_nk(&mut data, 0x600, "Shell", &[("MuiCache", 0x700)], &[]);
    write_nk(&mut data, 0x700, "MuiCache", &[], &[0x800, 0x880]);
    write_string_value(
        &mut data,
        0x800,
        "C:\\Windows\\System32\\cmd.exe",
        "Windows Command Processor",
        0x1000,
    );
    write_string_value(
        &mut data,
        0x880,
        "C:\\Windows\\System32\\notepad.exe",
        "Notepad",
        0x1100,
    );

    let entries = extract_muicache_from_usrclass_hive(&data, "Users/Test/UsrClass.dat").unwrap();
    assert_eq!(entries.len(), 2);
    let cmd = entries
        .iter()
        .find(|e| e.program_path == "C:\\Windows\\System32\\cmd.exe")
        .unwrap();
    assert_eq!(cmd.friendly_name, "Windows Command Processor");
    let notepad = entries
        .iter()
        .find(|e| e.program_path == "C:\\Windows\\System32\\notepad.exe")
        .unwrap();
    assert_eq!(notepad.friendly_name, "Notepad");
}

#[test]
fn extract_muicache_returns_empty_when_key_missing() {
    let data = empty_hive("USRCLASS");
    let entries = extract_muicache_from_usrclass_hive(&data, "Users/Test/UsrClass.dat").unwrap();
    assert!(entries.is_empty());
}
