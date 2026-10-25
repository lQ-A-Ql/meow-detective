#[path = "support/browser_hive.rs"]
mod fixture;

use artifacts_windows::registry::{
    reader::{BrowserOverlaySource, HiveView, ProvenanceSpan},
    RegistryHiveSource, RegistrySnapshot,
};
use std::{borrow::Cow, sync::Arc};

fn secret_hive() -> (Vec<u8>, Vec<u8>, u32) {
    let mut hive = fixture::Hive::new();
    let bytes: Vec<_> = (0..90_000).map(|index| (index % 251) as u8).collect();
    let value = hive.value("CurrVal", 3, &bytes, false);
    let secret = hive.key("Token", &[], &[value]);
    let secrets = hive.key("Secrets", &[secret], &[]);
    let policy = hive.key("Policy", &[secrets], &[]);
    let root = hive.key("ROOT", &[policy], &[]);
    (hive.finish(root), bytes, value)
}

#[test]
fn extraction_and_browsing_share_the_snapshot_and_segmented_value_data() {
    let (bytes, expected, value_offset) = secret_hive();
    let pointer = bytes.as_ptr();
    let snapshot = RegistrySnapshot::from_owned(bytes, None, None).unwrap();
    let first = snapshot.registry_view().unwrap();
    let second = snapshot.registry_view().unwrap();
    assert!(matches!(first, Cow::Borrowed(_)));
    assert!(std::ptr::eq(first.as_ref(), second.as_ref()));
    assert_eq!(snapshot.bytes().as_ptr(), pointer);
    let secrets =
        artifacts_windows::extract_lsa_secrets_from_security_hive(&snapshot, "SECURITY", None)
            .unwrap();
    assert_eq!(secrets.len(), 1);
    assert_eq!(secrets[0].encrypted_blob_hex, hex::encode(&expected));
    let view = snapshot.view().unwrap();
    let value = view.value(value_offset).unwrap();
    assert_eq!(
        view.value_range(&value, 16_340, 16).unwrap().bytes,
        expected[16_340..16_356]
    );
    let key = artifacts_windows::browse_registry_hive(&snapshot, "Policy\\Secrets\\Token").unwrap();
    assert_eq!(key.value_count, 1);
    assert_eq!(
        key.cell_offset,
        view.resolve("Policy\\Secrets\\Token").unwrap().offset
    );
}

#[test]
fn segmented_range_provenance_uses_the_actual_data_segments() {
    let (bytes, _, offset) = secret_hive();
    let bytes = Arc::new(bytes);
    let view = HiveView::open_with_provenance(
        bytes.clone(),
        vec![
            ProvenanceSpan {
                start: 4132 + 4096,
                end: 4132 + 8192,
            },
            ProvenanceSpan {
                start: 4132,
                end: 4132 + 5000,
            },
        ],
    )
    .unwrap();
    let value = view.value(offset).unwrap();
    assert_eq!(value.overlay_source, BrowserOverlaySource::Base);
    assert_eq!(
        view.value_range(&value, 0, 128).unwrap().overlay_source,
        BrowserOverlaySource::Merged
    );
    assert_eq!(
        view.value_range(&value, 20_000, 128)
            .unwrap()
            .overlay_source,
        BrowserOverlaySource::Base
    );
    let at = 4096 + offset as usize;
    let recovered = HiveView::open_with_provenance(
        bytes,
        vec![
            ProvenanceSpan {
                start: at + 16,
                end: at + 64,
            },
            ProvenanceSpan {
                start: 4132 + 4096,
                end: 4132 + 8192,
            },
            ProvenanceSpan {
                start: at,
                end: at + 32,
            },
            ProvenanceSpan {
                start: 4132,
                end: 4132 + 4096,
            },
        ],
    )
    .unwrap();
    let value = recovered.value(offset).unwrap();
    assert_eq!(value.overlay_source, BrowserOverlaySource::Recovered);
    assert_eq!(
        recovered
            .value_range(&value, 0, 128)
            .unwrap()
            .overlay_source,
        BrowserOverlaySource::Recovered
    );
}

#[test]
fn a_root_in_a_later_hbin_is_read_by_both_query_and_browser_interfaces() {
    use testing::builders::registry::cells;
    let mut bytes = vec![0; 3 * 4096];
    let root = 4096 + 32u32;
    cells::initialize(&mut bytes, root);
    bytes[4104..4108].copy_from_slice(&4096u32.to_le_bytes());
    bytes[8192..8224].fill(0);
    bytes[8192..8196].copy_from_slice(b"hbin");
    bytes[8196..8200].copy_from_slice(&4096u32.to_le_bytes());
    bytes[8200..8204].copy_from_slice(&4096u32.to_le_bytes());
    cells::allocate(&mut bytes, root, 0x54);
    let at = 4096 + root as usize;
    bytes[at + 4..at + 6].copy_from_slice(b"nk");
    bytes[at + 6..at + 8].copy_from_slice(&0x20u16.to_le_bytes());
    bytes[at + 0x4c..at + 0x4e].copy_from_slice(&4u16.to_le_bytes());
    bytes[at + 0x50..at + 0x54].copy_from_slice(b"ROOT");
    let snapshot = RegistrySnapshot::from_owned(bytes, None, None).unwrap();
    assert_eq!(snapshot.view().unwrap().root_key().unwrap().offset, root);
    let key = artifacts_windows::browse_registry_hive(&snapshot, "").unwrap();
    assert_eq!(key.cell_offset, root);
    assert_eq!(key.name, "ROOT");
}
