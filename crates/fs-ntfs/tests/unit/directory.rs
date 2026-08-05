use super::*;

fn index_entry(mft_ref: u64, name: &str, namespace: u8) -> Vec<u8> {
    let utf16 = name.encode_utf16().collect::<Vec<_>>();
    let entry_size = 0x52 + utf16.len() * 2;
    let mut entry = vec![0u8; entry_size];
    entry[0..8].copy_from_slice(&mft_ref.to_le_bytes());
    entry[8..10].copy_from_slice(&(entry_size as u16).to_le_bytes());
    entry[0x50] = utf16.len() as u8;
    entry[0x51] = namespace;
    for (index, character) in utf16.iter().enumerate() {
        let offset = 0x52 + index * 2;
        entry[offset..offset + 2].copy_from_slice(&character.to_le_bytes());
    }
    entry
}

fn parsed_entry(mft_ref: u64, name: &str, namespace: u8) -> DirEntry {
    parse_indx_entries(&index_entry(mft_ref, name, namespace))
        .pop()
        .expect("test INDX entry")
}

#[test]
fn index_entry_parses_filename_namespace() {
    let entry = parsed_entry((7u64 << 48) | 42, "Program Files", 1);

    assert_eq!(entry.namespace, FileNameNamespace::Win32);
    assert_eq!(entry.mft_ref, 42);
    assert_eq!(entry.mft_sequence, 7);
}

#[test]
fn modern_name_suppresses_dos_alias_for_the_same_reference() {
    let entries = vec![
        parsed_entry(42, "PROGRA~1", 2),
        parsed_entry(42, "Program Files", 1),
    ];

    let canonical = canonicalize_indx_entries(entries);

    assert_eq!(canonical.len(), 1);
    assert_eq!(canonical[0].node.name, "Program Files");
}

#[test]
fn dos_name_is_retained_when_it_is_the_only_alias() {
    let canonical = canonicalize_indx_entries(vec![parsed_entry(42, "LEGACY~1", 2)]);

    assert_eq!(canonical.len(), 1);
    assert_eq!(canonical[0].node.name, "LEGACY~1");
}

#[test]
fn duplicate_root_and_allocation_entries_collapse() {
    let entries = vec![
        parsed_entry(42, "Windows", 1),
        parsed_entry(42, "Windows", 1),
    ];

    let canonical = canonicalize_indx_entries(entries);

    assert_eq!(canonical.len(), 1);
    assert_eq!(canonical[0].node.name, "Windows");
}

#[test]
fn duplicate_name_across_namespaces_collapses() {
    let entries = vec![
        parsed_entry(42, "Windows", 0),
        parsed_entry(42, "Windows", 1),
    ];

    let canonical = canonicalize_indx_entries(entries);

    assert_eq!(canonical.len(), 1);
    assert_eq!(canonical[0].node.name, "Windows");
    assert_eq!(canonical[0].namespace, FileNameNamespace::Win32);
}

#[test]
fn distinct_non_dos_names_are_preserved_in_rank_order() {
    let entries = vec![
        parsed_entry(42, "PosixName", 0),
        parsed_entry(42, "WindowsName", 1),
        parsed_entry(42, "COMBIN~1", 3),
    ];

    let canonical = canonicalize_indx_entries(entries);
    let names = canonical
        .iter()
        .map(|entry| entry.node.name.as_str())
        .collect::<Vec<_>>();

    assert_eq!(names, vec!["WindowsName", "COMBIN~1", "PosixName"]);
}
