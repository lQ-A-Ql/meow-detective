use std::{fs::File, path::PathBuf, sync::atomic::AtomicBool};

use domain::DataSourceKind;
use evidence_access::{
    datasource_service::{ImageCollectionPlan, LvmDiscoverySource},
    mdraid::{discover_linux_evidence_set_mdraid, MdRaidDiscoveryMember},
};

#[test]
fn independently_complete_cloned_vgs_bind_only_their_own_physical_snapshots() {
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-workspaces");
    std::fs::create_dir_all(&scratch).unwrap();
    let workspace = tempfile::tempdir_in(scratch).unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/storage-combinations/md-lvm-ext4");
    let mut paths = Vec::new();
    for index in 0..2 {
        let path = workspace.path().join(format!("member-{index}.raw"));
        std::io::copy(
            &mut flate2::read::GzDecoder::new(
                File::open(fixture.join(format!("member-{index}.raw.gz"))).unwrap(),
            ),
            &mut File::create(&path).unwrap(),
        )
        .unwrap();
        paths.push(path);
    }
    let kind = DataSourceKind::Raw;
    let members = paths
        .iter()
        .map(|path| MdRaidDiscoveryMember {
            source_path: path,
            source_kind: &kind,
        })
        .collect::<Vec<_>>();
    let arrays = discover_linux_evidence_set_mdraid(&members).unwrap();
    assert_eq!(arrays.len(), 1);
    let first = workspace.path().join("clone-a.raw");
    let second = workspace.path().join("clone-b.raw");
    std::io::copy(
        &mut evidence_access::mdraid::open_mdraid_reader(&arrays[0]).unwrap(),
        &mut File::create(&first).unwrap(),
    )
    .unwrap();
    std::fs::copy(&first, &second).unwrap();
    let cancel = AtomicBool::new(false);
    let collection = ImageCollectionPlan::prepare(
        &[
            LvmDiscoverySource::new(&first, kind.clone()),
            LvmDiscoverySource::new(&second, kind),
        ],
        &cancel,
    )
    .unwrap();
    let discoveries = evidence_access::lvm::mapping_discovery_count();
    let volumes =
        evidence_access::lvm::discover_volumes_from_collection(&collection, &cancel).unwrap();
    assert_eq!(volumes.len(), 2);
    let candidate = collection.logical_volume_candidates()[0].clone();
    let identity = candidate.lvm_identity.as_ref().unwrap();
    let filesystem = evidence_access::file_service::open_source_filesystem(
        Box::new(
            evidence_core::RawImageReader::open(std::path::Path::new(
                &identity.pv_sources[0].source_path,
            ))
            .unwrap(),
        ),
        &candidate,
    )
    .unwrap();
    let expected = std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/public-small/mdraid/oracle.bin"),
    )
    .unwrap();
    assert_eq!(
        filesystem.read_file_range("evidence.bin", 0, 1024).unwrap(),
        expected[..1024]
    );
    drop(filesystem);
    let mut mismatched = candidate;
    let selected = mismatched.lvm_identity.as_mut().unwrap();
    selected.lv_uuid = "unavailable-volume-uuid".into();
    let reader = evidence_core::RawImageReader::open(std::path::Path::new(
        &selected.pv_sources[0].source_path,
    ))
    .unwrap();
    let error =
        evidence_access::file_service::open_source_filesystem(Box::new(reader), &mismatched)
            .err()
            .expect("unregistered LV UUID must be rejected");
    let evidence_access::file_service::FileServiceError::Io(error) = error else {
        panic!("expected LVM identity I/O error");
    };
    assert!(matches!(
        error.get_ref().and_then(|error| error.downcast_ref::<fs_lvm::LvmError>()),
        Some(fs_lvm::LvmError::MetadataParseError { message, .. })
            if message == "logical volume UUID is absent from the selected version"
    ));
    for volume in &volumes {
        for _ in 0..4 {
            drop(evidence_access::lvm::open_lvm_volume_reader(volume, &cancel).unwrap());
        }
    }
    assert_eq!(evidence_access::lvm::mapping_discovery_count(), discoveries);
    for volume in &volumes {
        assert_eq!(volume.sources.len(), 1);
        let own_path = std::fs::canonicalize(&volume.sources[0].source_path).unwrap();
        let other_path = if own_path == first.canonicalize().unwrap() {
            second.canonicalize().unwrap()
        } else {
            first.canonicalize().unwrap()
        };
        assert!(volume.source_snapshot.contains(&own_path));
        assert!(!volume.source_snapshot.contains(&other_path));
    }
    drop(collection);
    File::options()
        .write(true)
        .open(&first)
        .unwrap()
        .set_len(1)
        .unwrap();
    for volume in volumes {
        let belongs_to_first =
            PathBuf::from(&volume.sources[0].source_path) == first.canonicalize().unwrap();
        assert_eq!(volume.source_snapshot.validate().is_err(), belongs_to_first);
    }
}
