use std::collections::HashMap;
use std::io::{Cursor, Read, Seek, SeekFrom};
use std::path::PathBuf;

use app_services::ceph_reconstruction::{
    BluestoreDeviceOpener, CephFsObjectReadError, SourceBoundLvmError, SourceDbCephFsObjectReader,
    MAX_CEPHFS_OBJECT_RANGE_LENGTH,
};
use domain::DataSourceId;
use evidence_core::{EvidenceReader, ReaderInfo};

#[path = "support/cephfs_object_reader.rs"]
mod support;

struct SyntheticDeviceOpener {
    devices: HashMap<String, Vec<u8>>,
}

impl BluestoreDeviceOpener for SyntheticDeviceOpener {
    fn open(
        &self,
        _source_connection: &rusqlite::Connection,
        data_source_id: &DataSourceId,
        _inventory_id: &str,
    ) -> Result<Box<dyn EvidenceReader>, SourceBoundLvmError> {
        let bytes = self
            .devices
            .get(&data_source_id.0)
            .cloned()
            .ok_or(SourceBoundLvmError::BindingNotFound)?;
        Ok(Box::new(MemoryReader::new(bytes)))
    }
}

struct MemoryReader {
    cursor: Cursor<Vec<u8>>,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            info: ReaderInfo {
                path: PathBuf::from("synthetic-ceph-device"),
                size: bytes.len() as u64,
                kind: "synthetic-ceph-device".to_string(),
            },
            cursor: Cursor::new(bytes),
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        self.cursor.read(output)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.cursor.seek(position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

#[test]
fn complete_source_set_can_exceed_the_configured_replica_count() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = bindings(4);
    let descriptor = support::descriptor(&binding_refs(&bindings));
    let mut sources = Vec::new();
    let mut devices = HashMap::new();
    for (index, (source, inventory)) in bindings.iter().enumerate() {
        sources.push(support::write_source(
            &temp.path().join(format!("{source}.db")),
            &descriptor,
            source,
            inventory,
            (index < 3).then_some(support::OBJECT_SIZE),
        ));
        devices.insert(source.clone(), device(b"0123456789abcdef"));
    }
    let mut reader = SourceDbCephFsObjectReader::with_device_opener(
        descriptor,
        sources,
        3,
        Box::new(SyntheticDeviceOpener { devices }),
    )
    .unwrap();

    let range = reader.read_range(&support::locator(), 4, 6).unwrap();

    assert_eq!(range.bytes, b"456789");
    assert_eq!(range.provenance.len(), 3);
    assert_eq!(range.object_size, support::OBJECT_SIZE);

    let metadata = reader.inspect_object(&support::locator()).unwrap();
    assert_eq!(metadata.object_size, support::OBJECT_SIZE);
    assert_eq!(metadata.provenance.len(), 3);
}

#[test]
fn data_pool_reader_resolves_objects_directly_from_the_bound_semantic_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = bindings(1);
    let descriptor = support::descriptor_with_data_pool(&binding_refs(&bindings));
    let (source, inventory) = &bindings[0];
    let source_binding = support::write_data_source(
        &temp.path().join("data-source.db"),
        source,
        inventory,
        Some(support::OBJECT_SIZE),
    );
    let mut reader = SourceDbCephFsObjectReader::with_device_opener_for_pool(
        descriptor,
        vec![source_binding],
        1,
        support::DATA_POOL,
        Box::new(SyntheticDeviceOpener {
            devices: HashMap::from([(source.clone(), device(b"0123456789abcdef"))]),
        }),
    )
    .unwrap();

    let range = reader.read_range(&support::data_locator(), 4, 6).unwrap();

    assert_eq!(range.bytes, b"456789");
    assert_eq!(range.provenance.len(), 1);
}

#[test]
fn incomplete_replica_coverage_fails_closed() {
    let (mut reader, _temp) = reader_fixture(&[Some(16), Some(16), None], None);

    let error = reader.read_range(&support::locator(), 0, 4).unwrap_err();

    assert!(matches!(
        error,
        CephFsObjectReadError::ReplicaCoverageIncomplete {
            expected: 3,
            present: 2,
            ..
        }
    ));
}

#[test]
fn conflicting_replica_metadata_fails_before_returning_bytes() {
    let (mut reader, _temp) = reader_fixture(&[Some(16), Some(16), Some(17)], None);

    let error = reader.read_range(&support::locator(), 0, 4).unwrap_err();

    assert!(matches!(
        error,
        CephFsObjectReadError::MetadataConflict { .. }
    ));
}

#[test]
fn conflicting_replica_bytes_fail_closed() {
    let (mut reader, _temp) = reader_fixture(
        &[Some(16), Some(16), Some(16)],
        Some((2, b"0123456789abcdeX")),
    );

    let error = reader.read_range(&support::locator(), 12, 4).unwrap_err();

    assert!(matches!(error, CephFsObjectReadError::ByteConflict { .. }));
}

#[test]
fn stale_metadata_projection_is_rejected_against_the_current_semantic_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let bindings = bindings(1);
    let descriptor = support::descriptor(&binding_refs(&bindings));
    let (source, inventory) = &bindings[0];
    let path = temp.path().join("source.db");
    let source_binding = support::write_source(
        &path,
        &descriptor,
        source,
        inventory,
        Some(support::OBJECT_SIZE),
    );
    let conn = persistence_sqlite::open_existing_source(&path).unwrap();
    conn.execute(
        "UPDATE ceph_fs_metadata_inventories
         SET source_semantic_sha256 = ?1
         WHERE filesystem_identity = ?2 AND inventory_id = ?3",
        rusqlite::params!["f".repeat(64), descriptor.identity, inventory],
    )
    .unwrap();
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .unwrap();
    drop(conn);
    let mut reader = SourceDbCephFsObjectReader::with_device_opener(
        descriptor,
        vec![source_binding],
        1,
        Box::new(SyntheticDeviceOpener {
            devices: HashMap::from([(source.clone(), device(b"0123456789abcdef"))]),
        }),
    )
    .unwrap();

    let error = reader.read_range(&support::locator(), 0, 4).unwrap_err();

    assert!(matches!(
        error,
        CephFsObjectReadError::InventoryUnavailable { .. }
    ));
}

#[test]
fn range_limit_overflow_and_exact_eof_are_enforced() {
    let (mut reader, _temp) = reader_fixture(&[Some(16)], None);
    assert!(matches!(
        reader
            .read_range(&support::locator(), 0, MAX_CEPHFS_OBJECT_RANGE_LENGTH + 1)
            .unwrap_err(),
        CephFsObjectReadError::RangeTooLarge { .. }
    ));
    assert!(matches!(
        reader
            .read_range(&support::locator(), u64::MAX, 1)
            .unwrap_err(),
        CephFsObjectReadError::RangeOverflow { .. }
    ));
    assert!(matches!(
        reader.read_range(&support::locator(), 16, 1).unwrap_err(),
        CephFsObjectReadError::RangeOutOfBounds { .. }
    ));

    let eof = reader.read_range(&support::locator(), 16, 0).unwrap();
    assert!(eof.bytes.is_empty());
    assert_eq!(eof.offset, 16);
}

fn reader_fixture(
    object_sizes: &[Option<u64>],
    replacement: Option<(usize, &'static [u8])>,
) -> (SourceDbCephFsObjectReader, tempfile::TempDir) {
    let temp = tempfile::tempdir().unwrap();
    let bindings = bindings(object_sizes.len());
    let descriptor = support::descriptor(&binding_refs(&bindings));
    let mut sources = Vec::new();
    let mut devices = HashMap::new();
    for (index, ((source, inventory), object_size)) in bindings.iter().zip(object_sizes).enumerate()
    {
        sources.push(support::write_source(
            &temp.path().join(format!("{source}.db")),
            &descriptor,
            source,
            inventory,
            *object_size,
        ));
        let bytes = replacement
            .filter(|(replacement_index, _)| *replacement_index == index)
            .map(|(_, bytes)| bytes)
            .unwrap_or(b"0123456789abcdef");
        devices.insert(source.clone(), device(bytes));
    }
    let reader = SourceDbCephFsObjectReader::with_device_opener(
        descriptor,
        sources,
        object_sizes.len(),
        Box::new(SyntheticDeviceOpener { devices }),
    )
    .unwrap();
    (reader, temp)
}

fn device(object_bytes: &[u8]) -> Vec<u8> {
    let mut device = vec![0; support::DEVICE_SIZE];
    let end = support::PHYSICAL_OFFSET + object_bytes.len();
    device[support::PHYSICAL_OFFSET..end].copy_from_slice(object_bytes);
    device
}

fn bindings(count: usize) -> Vec<(String, String)> {
    (0..count)
        .map(|index| (format!("source-{index}"), format!("inventory-{index}")))
        .collect()
}

fn binding_refs(bindings: &[(String, String)]) -> Vec<(&str, &str)> {
    bindings
        .iter()
        .map(|(source, inventory)| (source.as_str(), inventory.as_str()))
        .collect()
}
