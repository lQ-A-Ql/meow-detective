use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use domain::DataSourceKind;
use evidence_access::mdraid::{
    discover_linux_evidence_set_mdraid, open_mdraid_filesystem, open_mdraid_reader,
    probe_mdraid_filesystems, write_mdraid_descriptor, MdRaidDiscoveryMember,
};

#[test]
fn native_md_ext4_uses_shared_filesystem_discovery_and_bounded_reads() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/mdraid");
    let workspace = tempfile::tempdir().unwrap();
    let paths = (0..2)
        .map(|index| {
            let path = workspace.path().join(format!("member-{index}.raw"));
            let compressed = File::open(
                root.join("raid0-ext4")
                    .join(format!("member-{index}.raw.gz")),
            )
            .unwrap();
            std::io::copy(
                &mut flate2::read::GzDecoder::new(compressed),
                &mut File::create(&path).unwrap(),
            )
            .unwrap();
            path
        })
        .collect::<Vec<_>>();
    let kind = DataSourceKind::Raw;
    let members = paths
        .iter()
        .map(|path| MdRaidDiscoveryMember {
            source_path: path,
            source_kind: &kind,
        })
        .collect::<Vec<_>>();
    let plans = discover_linux_evidence_set_mdraid(&members).unwrap();
    assert_eq!(plans.len(), 1);
    let mut changed_plan = plans[0].clone();
    changed_plan.layout += 1;
    assert!(open_mdraid_reader(&changed_plan).is_err());
    let descriptor = write_mdraid_descriptor(workspace.path(), &plans[0]).unwrap();
    let probe = probe_mdraid_filesystems(&descriptor).unwrap();
    assert_eq!(probe.candidates.len(), 1);
    assert_eq!(
        probe.candidates[0].kind,
        evidence_access::datasource_service::ImageFilesystemKind::Ext4
    );
    let filesystem = open_mdraid_filesystem(&descriptor, &probe.candidates[0]).unwrap();
    let expected = std::fs::read(root.join("oracle.bin")).unwrap();
    assert_eq!(
        filesystem
            .read_file_range("evidence.bin", 4093, 8177)
            .unwrap(),
        expected[4093..4093 + 8177]
    );
    let mut file = evidence_access::file_service::open_filesystem_range_file(
        filesystem,
        "evidence.bin".to_string(),
        expected.len() as u64,
    )
    .unwrap();
    file.seek(SeekFrom::Start(7331)).unwrap();
    let mut actual = [0; 73];
    file.read_exact(&mut actual).unwrap();
    assert_eq!(actual, expected[7331..7331 + 73]);
    drop(file);
    let mut changed = std::fs::OpenOptions::new()
        .write(true)
        .open(&paths[0])
        .unwrap();
    changed.seek(SeekFrom::Start(4096 + 32)).unwrap();
    changed.write_all(b"x").unwrap();
    changed.sync_all().unwrap();
    assert!(open_mdraid_reader(&plans[0]).is_err());
}

#[test]
fn native_lvm_members_include_a_pv_without_metadata_and_preserve_file_bytes() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures/public-small");
    let fixture = root.join("lvm-native/mdraid-ext4");
    let workspace = tempfile::tempdir().unwrap();
    let mut paths = Vec::new();
    for index in 0..2 {
        let path = workspace.path().join(format!("pv-{index}.raw"));
        std::io::copy(
            &mut flate2::read::GzDecoder::new(
                File::open(fixture.join(format!("pv-{index}.raw.gz"))).unwrap(),
            ),
            &mut File::create(&path).unwrap(),
        )
        .unwrap();
        paths.push(path);
    }
    let mut metadata_free =
        evidence_access::datasource_service::open_evidence_reader(&paths[1], &DataSourceKind::Raw)
            .unwrap();
    let label = fs_lvm::label::parse_pv_label(&mut metadata_free, 0).unwrap();
    assert!(label.metadata_areas.is_empty());
    let kind = DataSourceKind::Raw;
    let members = paths
        .iter()
        .map(|path| MdRaidDiscoveryMember {
            source_path: path,
            source_kind: &kind,
        })
        .collect::<Vec<_>>();
    let plans = discover_linux_evidence_set_mdraid(&members).unwrap();
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].members.len(), 2);
    assert!(plans[0].members.iter().all(|member| matches!(
        &member.origin,
        evidence_access::mdraid::MdRaidMemberOrigin::Lvm { identity, .. }
            if identity.pv_sources.len() == 2
                && identity.pv_sources[0].source_path != identity.pv_sources[1].source_path
    )));
    assert_eq!(
        open_mdraid_reader(&plans[0]).unwrap().logical_size(),
        14 * 1024 * 1024
    );
    let descriptor = write_mdraid_descriptor(workspace.path(), &plans[0]).unwrap();
    let probe = probe_mdraid_filesystems(&descriptor).unwrap();
    assert_eq!(probe.candidates.len(), 1);
    let filesystem = open_mdraid_filesystem(&descriptor, &probe.candidates[0]).unwrap();
    let expected = std::fs::read(root.join("mdraid/oracle.bin")).unwrap();
    assert_eq!(
        filesystem
            .read_file_range("evidence.bin", 0, expected.len())
            .unwrap(),
        expected
    );
}
