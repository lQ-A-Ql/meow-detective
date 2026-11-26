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
    let plans = discover_linux_evidence_set_mdraid(workspace.path(), &members).unwrap();
    assert_eq!(plans.len(), 1);
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
