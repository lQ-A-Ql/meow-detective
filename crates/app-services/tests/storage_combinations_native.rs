use app_services::{case_service, cluster_service, file_service, source_db};
use domain::{
    structure::{AcquisitionId, SourceRevisionContent},
    DataSource, DataSourceId, DataSourceKind,
};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::PathBuf, sync::atomic::AtomicBool};

fn exercise(name: &str, filesystem: &str) {
    let fixtures =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures/public-small");
    let scratch = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-workspaces");
    std::fs::create_dir_all(&scratch).unwrap();
    let workspace = tempfile::tempdir_in(scratch).unwrap();
    let folder = fixtures.join("storage-combinations").join(name);
    let paths = (0..2)
        .map(|index| {
            let path = workspace.path().join(format!("member-{index}.raw"));
            std::io::copy(
                &mut flate2::read::GzDecoder::new(
                    File::open(folder.join(format!("member-{index}.raw.gz"))).unwrap(),
                ),
                &mut File::create(&path).unwrap(),
            )
            .unwrap();
            path
        })
        .collect::<Vec<_>>();
    let kind = DataSourceKind::Raw;
    let members = paths
        .iter()
        .map(|path| evidence_access::mdraid::MdRaidDiscoveryMember {
            source_path: path,
            source_kind: &kind,
        })
        .collect::<Vec<_>>();
    let arrays = cluster_service::discover_linux_evidence_set_mdraid(&members).unwrap();
    assert_eq!(arrays.len(), 1);
    let array = &arrays[0];
    let mut logical = cluster_service::open_mdraid_reader(array).unwrap();
    let mut digest = Sha256::new();
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        let count = logical.read(&mut bytes).unwrap();
        if count == 0 {
            break;
        }
        digest.update(&bytes[..count]);
    }
    let native_digest = std::fs::read_to_string(folder.join("logical.sha256")).unwrap();
    assert_eq!(
        hex::encode(digest.finalize()),
        native_digest.split_whitespace().next().unwrap()
    );
    drop(logical);
    let case = case_service::create_case(workspace.path(), name, None).unwrap();
    let connection = case.connection().unwrap();
    let mut inputs = Vec::new();
    for (index, content) in array.source_content.iter().enumerate() {
        let id = DataSourceId(format!("physical-{index}"));
        DataSourceRepo::new(&connection)
            .insert(
                &case.meta.id,
                &DataSource {
                    id: id.clone(),
                    name: id.0.clone(),
                    kind: content.source_kind.clone(),
                    source_path: PathBuf::from(&content.source_path),
                    imported_at: chrono::Utc::now(),
                    provenance: Default::default(),
                },
            )
            .unwrap();
        let reference = SourceRevisionRepo::new(&connection)
            .bind(
                &case.meta.id.0,
                &id.0,
                &SourceRevisionContent::captured(
                    AcquisitionId(format!("capture-{index}")),
                    content.logical_size,
                    None,
                    content.logical_sha256.clone(),
                ),
                true,
            )
            .unwrap();
        let control = source_db::open_source_db(&case.case_root, &id).unwrap();
        evidence_access::source_runtime::content::persist_registered_content(
            &control, &reference, content,
        )
        .unwrap();
        inputs.push(reference);
    }
    let mut foreign = inputs.clone();
    foreign[0].case_id = uuid::Uuid::new_v4().to_string();
    assert!(cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        array,
        &foreign
    )
    .is_err());
    let source = cluster_service::materialize_mdraid_array(
        &connection,
        &case.case_root,
        &case.meta.id,
        array,
        &inputs,
    )
    .unwrap();
    let native =
        source_db::open_registered_fjall_source_read_only(&case.case_root, &source.id).unwrap();
    let partitions = native
        .partition_list_by_source(
            &native.snapshot(),
            &source.id.0,
            None,
            persistence_source::fjall::ScanDirection::Forward,
            16,
        )
        .unwrap();
    let partition = partitions
        .records
        .iter()
        .find(|partition| partition.filesystem.as_deref() == Some(filesystem))
        .unwrap();
    let source_connection =
        source_db::open_registered_source_db(&connection, &case.case_root, &source.id).unwrap();
    let file = FileRepo::new(&source_connection)
        .find_by_partition_and_path(
            &source.id,
            partition.partition_index as usize,
            "evidence.bin",
        )
        .unwrap()
        .unwrap();
    let expected = std::fs::read(fixtures.join("mdraid/oracle.bin")).unwrap();
    assert_eq!(
        file_service::FileHeaderReadCache::new(case.meta.id.0.clone())
            .read_file_header_by_id(&source_connection, &file.id, 512)
            .unwrap(),
        expected[..512]
    );
    let mut context = file_service::SourceReadContext::new(
        &source_connection,
        &connection,
        &case.case_root,
        &case.meta.id,
        &source.id,
    );
    let mut reader = match context.open_file_range_by_id(&file.id).unwrap() {
        file_service::RangeContentReader::Seekable(reader) => reader,
        _ => panic!("native filesystem content must be seekable"),
    };
    let mut actual = Vec::new();
    reader.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, expected);
    let range = file_service::read_file_bytes_for_case(&mut context, &file.id, 65530, 27).unwrap();
    assert_eq!(range, expected[65530..65557]);
    let request = app_services::structure_analysis::StructureAnalysisRequest {
        case_id: case.meta.id.clone(),
        source_ids: vec![source.id.0.clone()],
        intent: domain::structure::ReadIntent::MetadataInspection,
        priority: domain::structure::ReadPriority::Interactive,
        consumers: vec!["native-combination".into()],
        attempt: None,
        required_features: Default::default(),
    };
    let run = app_services::structure_analysis::run_structure_analysis(
        &connection,
        &case.case_root,
        &request,
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(run
        .work_results
        .iter()
        .all(|result| result.bytes_read == result.receipt.ranges.covered_bytes()));
    assert!(!run.plan.graph.mappings.is_empty());
}

#[test]
fn native_md_gpt_ext4_matches_full_disk_and_file_oracles() {
    exercise("md-gpt-ext4", "Ext4");
}

#[test]
fn native_md_lvm_ext4_matches_full_disk_and_file_oracles() {
    exercise("md-lvm-ext4", "Ext4");
}

#[test]
fn native_md_xfs_matches_full_disk_and_file_oracles() {
    exercise("md-xfs", "XFS");
}

#[test]
fn native_md_btrfs_matches_full_disk_and_file_oracles() {
    exercise("md-btrfs", "Btrfs");
}
