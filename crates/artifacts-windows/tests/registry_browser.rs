#[path = "support/browser_hive.rs"]
mod fixture;
use artifacts_windows::registry::browser::{BrowserError, HiveSnapshot};
use std::sync::Arc;

fn example() -> (HiveSnapshot, Vec<u8>, u32) {
    let mut h = fixture::Hive::new();
    let big: Vec<_> = (0..90000).map(|i| (i % 251) as u8).collect();
    let binary = h.value("large", 3, &big, false);
    let qword = h.value("precise", 11, &u64::MAX.to_le_bytes(), false);
    let empty = h.value("", 3, &[], true);
    let expand = h.value(
        "environment",
        2,
        &"%SystemRoot%\0"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
        false,
    );
    let big_endian = h.value("BE", 5, &42u32.to_be_bytes(), true);
    let child = h.key(
        "CurrentVersion",
        &[],
        &[binary, qword, empty, expand, big_endian],
    );
    let root = h.key("ROOT", &[child], &[]);
    (
        HiveSnapshot::open(Arc::new(h.finish(root))).unwrap(),
        big,
        child,
    )
}

#[test]
fn paged_keys_ignore_hash_words_and_resolve_case_insensitively() {
    let (h, _, child) = example();
    let root = h.root_key().unwrap();
    let page = h.subkeys(&root, 0, 1).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.declared, 1);
    assert!(page.diagnostic.is_none());
    assert_eq!(h.resolve("currentversion").unwrap().offset, child);
    assert_eq!(h.resolve("missing").unwrap_err(), BrowserError::NotFound);
}
#[test]
fn all_value_types_zero_length_and_default_names_survive_pagination() {
    let (h, _, _) = example();
    let key = h.resolve("CurrentVersion").unwrap();
    let p = h.values(&key, 0, 2).unwrap();
    assert_eq!(p.next, Some(2));
    assert_eq!(p.declared, 5);
    let v = p.items[1].1.as_ref().unwrap();
    assert_eq!(v.preview, u64::MAX.to_string());
    let p = h.values(&key, 2, 3).unwrap();
    assert_eq!(p.items[0].1.as_ref().unwrap().name, "");
    assert_eq!(p.items[1].1.as_ref().unwrap().preview, "%SystemRoot%");
    assert_eq!(p.items[2].1.as_ref().unwrap().preview, "42");
}
#[test]
fn segmented_ranges_equal_source_across_boundaries_and_eof() {
    let (h, bytes, _) = example();
    let key = h.resolve("CurrentVersion").unwrap();
    let p = h.values(&key, 0, 1).unwrap();
    let v = p.items[0].1.as_ref().unwrap();
    for (at, len) in [(0, 65536), (16340, 20), (65536, 65536), (90000, 32)] {
        let r = h.value_range(v, at, len).unwrap();
        assert_eq!(
            r.bytes,
            bytes[at as usize..(at as usize + len).min(bytes.len())]
        );
    }
    assert_eq!(h.value_range(v, 90001, 1).unwrap_err(), BrowserError::Limit);
}
#[test]
fn invalid_headers_and_free_or_interior_cells_are_rejected() {
    assert_eq!(
        HiveSnapshot::open(Arc::new(vec![0; 8192])).unwrap_err(),
        BrowserError::Corrupt
    );
    let (h, _, child) = example();
    assert_eq!(
        h.key(child + 8, String::new()).unwrap_err(),
        BrowserError::Corrupt
    );
    assert_eq!(
        h.key(0xfffffff0, String::new()).unwrap_err(),
        BrowserError::Corrupt
    );
}
#[test]
fn corrupt_items_remain_visible_with_errors() {
    let mut h = fixture::Hive::new();
    let child = h.key("child", &[], &[]);
    let root = h.key("root", &[child, 0xfffffff0], &[0xfffffff0]);
    let h = HiveSnapshot::open(Arc::new(h.finish(root))).unwrap();
    let page = h.subkeys(&h.root_key().unwrap(), 0, 10).unwrap();
    assert!(page.items[1].1.is_err());
    assert!(h.values(&h.root_key().unwrap(), 0, 10).unwrap().items[0]
        .1
        .is_err());
}
#[test]
fn paths_and_range_allocations_are_bounded() {
    let (h, _, _) = example();
    assert_eq!(
        h.resolve("bad\0path").unwrap_err(),
        BrowserError::InvalidPath
    );
    assert_eq!(
        h.resolve(&vec!["x"; 65].join("\\")).unwrap_err(),
        BrowserError::Limit
    );
    let v = h
        .values(&h.resolve("CurrentVersion").unwrap(), 0, 1)
        .unwrap()
        .items
        .remove(0)
        .1
        .unwrap();
    assert_eq!(
        h.value_range(&v, 0, 65537).unwrap_err(),
        BrowserError::Limit
    );
}
