use app_services::datasource_service::{
    ImageFilesystemCandidate, ImageFilesystemKind, ImageFilesystemSource,
};
use app_services::source_runtime::{
    open_factory, PartitionReadPlan, SourceIdentity, SourceRuntime,
};
use domain::{DataSourceId, DataSourceKind};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

#[test]
fn raw_factory_reopens_independent_cursors_with_stable_size() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, b"0123456789").unwrap();
    let factory = open_factory(&path, &DataSourceKind::Raw).unwrap();
    assert_eq!(factory.logical_size(), 10);
    assert!(factory.capabilities().independent_cursor);
    let mut first = factory.open_reader().unwrap();
    first.seek(SeekFrom::Start(4)).unwrap();
    let mut value = [0; 2];
    first.read_exact(&mut value).unwrap();
    let mut second = factory.open_reader().unwrap();
    let mut head = [0; 2];
    second.read_exact(&mut head).unwrap();
    assert_eq!(&value, b"45");
    assert_eq!(&head, b"01");
}

#[test]
fn runtime_keeps_source_identity_when_its_generation_tag_changes() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("source.raw");
    std::fs::write(&path, vec![0; 512]).unwrap();
    let factory = open_factory(&path, &DataSourceKind::Raw).unwrap();
    let identity = SourceIdentity {
        case_id: "case".into(),
        data_source_id: DataSourceId("source".into()),
        revision: 7,
        source_kind: DataSourceKind::Raw,
        path: PathBuf::from(&path),
    };
    let mut runtime = SourceRuntime::new(identity, factory);
    assert_eq!(runtime.generation(), 0);
    runtime.bump_generation();
    assert_eq!(runtime.generation(), 1);
    assert_eq!(runtime.identity().revision, 7);
    assert_eq!(runtime.open_reader().unwrap().info().size, 512);
}

#[test]
fn partition_plan_preserves_offsets_lengths_and_lvm_identity() {
    let candidate = ImageFilesystemCandidate {
        partition_index: Some(3),
        partition_name: Some("root".into()),
        kind: ImageFilesystemKind::Ntfs,
        offset: 4096,
        length: Some(8192),
        source: ImageFilesystemSource::GptPartition,
        lvm_identity: None,
    };
    let plan = PartitionReadPlan::from_candidate(3, &candidate, DataSourceKind::Raw);
    assert_eq!(plan.offset, 4096);
    assert_eq!(plan.length, Some(8192));
    assert!(plan.is_direct_ntfs());
}

#[test]
fn unsupported_logical_source_has_no_host_reader_factory() {
    let temp = tempfile::TempDir::new().unwrap();
    assert!(open_factory(temp.path(), &DataSourceKind::LogicalDirectory).is_err());
}

#[test]
fn raw_factory_rejects_a_changed_source_member() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("changing.raw");
    std::fs::write(&path, vec![0; 512]).unwrap();
    let factory = open_factory(&path, &DataSourceKind::Raw).unwrap();
    std::fs::write(&path, vec![0; 1024]).unwrap();
    assert!(factory.open_reader().is_err());
}

#[test]
fn raw_factory_rejects_a_changed_qcow_backing_member_even_when_child_is_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("parent.raw");
    std::fs::write(&parent, b"parent payload").unwrap();
    let child = temp.path().join("child.qcow2");
    let mut bytes = vec![0u8; 1024];
    bytes[..4].copy_from_slice(b"QFI\xfb");
    bytes[4..8].copy_from_slice(&3u32.to_be_bytes());
    bytes[8..16].copy_from_slice(&72u64.to_be_bytes());
    bytes[16..20].copy_from_slice(&10u32.to_be_bytes());
    bytes[20..24].copy_from_slice(&9u32.to_be_bytes());
    bytes[24..32].copy_from_slice(&14u64.to_be_bytes());
    bytes[36..40].copy_from_slice(&1u32.to_be_bytes());
    bytes[40..48].copy_from_slice(&512u64.to_be_bytes());
    bytes[72..82].copy_from_slice(b"parent.raw");
    std::fs::write(&child, &bytes).unwrap();
    let factory = open_factory(&child, &DataSourceKind::Raw).unwrap();
    factory.validate_snapshot().unwrap();
    let mut reader = factory.open_reader().unwrap();
    let mut output = [0; 14];
    reader.read_exact(&mut output).unwrap();
    assert_eq!(&output, b"parent payload");
    std::fs::write(&parent, b"changed parent payload").unwrap();
    assert!(factory.validate_snapshot().is_err());
    assert!(factory.open_reader().is_err());
    std::fs::remove_file(&parent).unwrap();
    assert!(factory.validate_snapshot().is_err());
    assert_eq!(std::fs::read(&child).unwrap(), bytes);
}
