use super::*;
use crate::registry::tests::*;

fn make_pidl_blob_with_path(path: &str) -> Vec<u8> {
    let mut blob = vec![0x14, 0x00, 0x1f, 0x00, 0xe0, 0x00];
    let utf16: Vec<u8> = path.encode_utf16().flat_map(u16::to_le_bytes).collect();
    blob.extend_from_slice(&utf16);
    blob.extend_from_slice(&[0x00, 0x00]);
    blob.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]);
    blob
}

#[test]
fn extract_shellbags_from_fixture() {
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
    write_nk(&mut data, 0x600, "Shell", &[("BagMRU", 0x700)], &[]);
    write_nk(&mut data, 0x700, "BagMRU", &[("0", 0x800)], &[0x900]);
    write_dword_value(&mut data, 0x900, "NodeSlot", 7);
    write_nk(&mut data, 0x800, "0", &[("0", 0xa00)], &[]);
    write_nk(&mut data, 0xa00, "0", &[], &[0xb00, 0xc00]);
    write_dword_value(&mut data, 0xc00, "NodeSlot", 7);
    let pidl = make_pidl_blob_with_path("C:\\Users\\Test\\Documents");
    write_binary_value(&mut data, 0xb00, "", &pidl, 0x4000);

    let entries = extract_shellbags_from_usrclass_hive(&data, "Users/Test/UsrClass.dat").unwrap();
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.path, "C:\\Users\\Test\\Documents");
    assert_eq!(entry.raw_pidl_hex, hex::encode(&pidl));
    assert_eq!(entry.node_slot, Some(7));
    assert!(entry.source_key_path.ends_with("BagMRU\\0\\0"));
}

#[test]
fn extract_shellbags_returns_empty_when_bagmru_missing() {
    let data = empty_hive("USRCLASS");
    let entries = extract_shellbags_from_usrclass_hive(&data, "Users/Test/UsrClass.dat").unwrap();
    assert!(entries.is_empty());
}
