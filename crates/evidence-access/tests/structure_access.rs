use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

use domain::{
    structure::{ByteRange, ReadIntent, ReadPriority, ReadVerification, SourceRevisionRef},
    DataSourceKind,
};
use evidence_access::{
    datasource_service::{CollectionPlanError, ImageCollectionPlan, LvmDiscoverySource},
    mdraid::discover_mdraid_from_collection,
    structure_access::{
        open_verified_view, plan_target_access, AccessTarget, TargetAccessError,
        TargetAccessRequest,
    },
};
use evidence_core::EvidenceReader;
use sha2::{Digest, Sha256};

fn native_members(workspace: &Path) -> Vec<PathBuf> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/mdraid/raid0-v12");
    (0..2)
        .map(|index| {
            let path = workspace.join(format!("member-{index}.raw"));
            let compressed = File::open(root.join(format!("member-{index}.raw.gz"))).unwrap();
            std::io::copy(
                &mut flate2::read::GzDecoder::new(compressed),
                &mut File::create(&path).unwrap(),
            )
            .unwrap();
            path
        })
        .collect()
}

fn workspace() -> tempfile::TempDir {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-structure-access");
    std::fs::create_dir_all(&root).unwrap();
    tempfile::Builder::new()
        .prefix("native-")
        .tempdir_in(root)
        .unwrap()
}

fn source_revision(source_id: &str) -> SourceRevisionRef {
    SourceRevisionRef {
        case_id: "case-access".into(),
        source_id: source_id.into(),
        revision: 1,
    }
}

fn request<'a>(
    target: AccessTarget<'a>,
    source_id: &str,
    ranges: Option<Vec<ByteRange>>,
    intent: ReadIntent,
) -> TargetAccessRequest<'a> {
    TargetAccessRequest {
        target,
        source_revision: source_revision(source_id),
        ranges,
        intent,
        priority: ReadPriority::Interactive,
        deadline_unix_ms: None,
        reorderable: true,
        consumers: vec!["structure-access-test".into()],
        verification: ReadVerification::ContentDigest,
        attempt: None,
    }
}

fn oracle() -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fixtures/public-small/mdraid/oracle.bin"),
    )
    .unwrap()
}

#[test]
fn native_md_ranges_match_oracle_have_independent_cursors_and_verify_digest() {
    let workspace = workspace();
    let paths = native_members(workspace.path());
    let sources = paths
        .iter()
        .map(|path| LvmDiscoverySource::new(path, DataSourceKind::Raw))
        .collect::<Vec<_>>();
    let cancel = AtomicBool::new(false);
    let collection = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    let native = discover_mdraid_from_collection(&collection, &cancel).unwrap();
    let expected = oracle();
    let range = ByteRange::new(4093, 8177).unwrap();
    let access = plan_target_access(
        request(
            AccessTarget::MdRaid(&native[0]),
            "md-access",
            Some(vec![range]),
            ReadIntent::ContentRead,
        ),
        &cancel,
    )
    .unwrap();
    assert_eq!(access.demand().ranges.ranges(), &[range]);
    assert!(access.demand().ranges.target_size() >= range.end());
    let mut view = open_verified_view(access, &cancel).unwrap();
    assert_eq!(view.info().size, range.length());
    let mut actual = Vec::new();
    view.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, expected[4093..12270]);
    view.seek(SeekFrom::End(-1)).unwrap();
    let mut last = [0; 2];
    assert_eq!(view.read(&mut last).unwrap(), 1);
    assert_eq!(last[0], expected[12269]);
    let second = plan_target_access(
        request(
            AccessTarget::MdRaid(&native[0]),
            "md-access",
            Some(vec![range]),
            ReadIntent::ContentRead,
        ),
        &cancel,
    )
    .unwrap();
    let mut second = open_verified_view(second, &cancel).unwrap();
    let mut first = [0; 17];
    second.read_exact(&mut first).unwrap();
    assert_eq!(first, expected[4093..4110]);
    assert_eq!(view.stream_position().unwrap(), range.length());
    drop(second);
    drop(view);
    let expected_digest = hex::encode(Sha256::digest(&expected[4093..12270]));
    let verification = plan_target_access(
        request(
            AccessTarget::MdRaid(&native[0]),
            "md-access",
            Some(vec![range]),
            ReadIntent::ContentRead,
        ),
        &cancel,
    )
    .unwrap();
    let mut verified = open_verified_view(verification, &cancel).unwrap();
    let mut actual = Vec::new();
    verified.read_to_end(&mut actual).unwrap();
    assert_eq!(hex::encode(Sha256::digest(&actual)), expected_digest);
    drop(verified);
    drop(collection);
    workspace.close().unwrap();
}

#[test]
fn multiple_ranges_are_merged_and_read_as_one_ordered_view() {
    let workspace = workspace();
    let paths = native_members(workspace.path());
    let sources = paths
        .iter()
        .map(|path| LvmDiscoverySource::new(path, DataSourceKind::Raw))
        .collect::<Vec<_>>();
    let cancel = AtomicBool::new(false);
    let collection = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    let native = discover_mdraid_from_collection(&collection, &cancel).unwrap();
    let expected = oracle();
    let ranges = vec![
        ByteRange::new(300, 20).unwrap(),
        ByteRange::new(0, 10).unwrap(),
        ByteRange::new(315, 10).unwrap(),
    ];
    let access = plan_target_access(
        request(
            AccessTarget::MdRaid(&native[0]),
            "md-access",
            Some(ranges),
            ReadIntent::ContentRead,
        ),
        &cancel,
    )
    .unwrap();
    assert_eq!(
        access.demand().ranges.ranges(),
        &[
            ByteRange::new(0, 10).unwrap(),
            ByteRange::new(300, 25).unwrap()
        ]
    );
    let mut view = open_verified_view(access, &cancel).unwrap();
    assert_eq!(view.info().size, 35);
    let mut actual = vec![0; 35];
    view.read_exact(&mut actual).unwrap();
    let mut expected_view = expected[..10].to_vec();
    expected_view.extend_from_slice(&expected[300..325]);
    assert_eq!(actual, expected_view);
    view.seek(SeekFrom::Start(10)).unwrap();
    let mut second = vec![0; 5];
    view.read_exact(&mut second).unwrap();
    assert_eq!(second, expected[300..305]);
    assert!(view.seek(SeekFrom::Start(36)).is_err());
    drop(collection);
    workspace.close().unwrap();
}

#[test]
fn collection_ranges_reject_unknown_members_bounds_cancellation_and_content_changes() {
    let workspace = workspace();
    let paths = native_members(workspace.path());
    let sources = paths
        .iter()
        .map(|path| LvmDiscoverySource::new(path, DataSourceKind::Raw))
        .collect::<Vec<_>>();
    let cancel = AtomicBool::new(false);
    let collection = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    let kind = DataSourceKind::Raw;
    let target = || AccessTarget::CollectionSource {
        collection: &collection,
        path: &paths[0],
        kind: &kind,
    };
    let oversized = ByteRange::new(0, std::fs::metadata(&paths[0]).unwrap().len() + 1).unwrap();
    assert!(matches!(
        plan_target_access(
            request(
                target(),
                "collection-source",
                Some(vec![oversized]),
                ReadIntent::ContentRead
            ),
            &cancel
        ),
        Err(TargetAccessError::Range(_))
    ));
    assert!(matches!(
        plan_target_access(
            request(target(), "collection-source", None, ReadIntent::ContentRead),
            &AtomicBool::new(true)
        ),
        Err(TargetAccessError::Cancelled)
    ));
    let unknown = workspace.path().join("unknown.raw");
    std::fs::copy(&paths[0], &unknown).unwrap();
    assert!(matches!(
        plan_target_access(
            request(
                AccessTarget::CollectionSource {
                    collection: &collection,
                    path: &unknown,
                    kind: &kind
                },
                "collection-source",
                None,
                ReadIntent::ContentRead
            ),
            &cancel
        ),
        Err(TargetAccessError::Collection(
            CollectionPlanError::MemberMismatch
        ))
    ));
    let range = ByteRange::new(4093, 8177).unwrap();
    let access = plan_target_access(
        request(
            target(),
            "collection-source",
            Some(vec![range]),
            ReadIntent::ContentRead,
        ),
        &cancel,
    )
    .unwrap();
    let mut view = open_verified_view(access, &cancel).unwrap();
    let mut expected = File::open(&paths[0]).unwrap();
    expected.seek(SeekFrom::Start(range.start())).unwrap();
    let mut expected_bytes = vec![0; range.length() as usize];
    expected.read_exact(&mut expected_bytes).unwrap();
    let mut actual = Vec::new();
    view.read_to_end(&mut actual).unwrap();
    assert_eq!(actual, expected_bytes);
    drop(expected);
    drop(view);
    let access = plan_target_access(
        request(
            target(),
            "collection-source",
            Some(vec![range]),
            ReadIntent::ContentRead,
        ),
        &cancel,
    )
    .unwrap();
    let mut view = open_verified_view(access, &cancel).unwrap();
    use std::io::Write;
    let mut changed = std::fs::OpenOptions::new()
        .write(true)
        .open(&paths[0])
        .unwrap();
    changed.seek(SeekFrom::Start(range.start())).unwrap();
    changed.write_all(&[expected_bytes[0] ^ 0xff]).unwrap();
    changed.flush().unwrap();
    assert!(view.read(&mut [0; 1]).is_err());
    drop(changed);
    drop(view);
    drop(collection);
    workspace.close().unwrap();
}

#[test]
fn md_full_view_empty_eof_and_read_cancellation_preserve_boundaries() {
    let workspace = workspace();
    let paths = native_members(workspace.path());
    let sources = paths
        .iter()
        .map(|path| LvmDiscoverySource::new(path, DataSourceKind::Raw))
        .collect::<Vec<_>>();
    let cancel = AtomicBool::new(false);
    let collection = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    let native = discover_mdraid_from_collection(&collection, &cancel).unwrap();
    let access = plan_target_access(
        request(
            AccessTarget::MdRaid(&native[0]),
            "md-access",
            None,
            ReadIntent::MetadataInspection,
        ),
        &cancel,
    )
    .unwrap();
    let size = access.demand().ranges.target_size();
    assert!(access.demand().ranges.is_full());
    let mut full = open_verified_view(access, &cancel).unwrap();
    assert_eq!(full.info().size, size);
    assert_eq!(full.seek(SeekFrom::End(0)).unwrap(), size);
    assert_eq!(full.read(&mut [0; 1]).unwrap(), 0);
    assert!(full.seek(SeekFrom::Start(0)).is_ok());
    assert_eq!(full.read(&mut []).unwrap(), 0);
    let empty = plan_target_access(
        request(
            AccessTarget::MdRaid(&native[0]),
            "md-access",
            Some(vec![ByteRange::new(size, 0).unwrap()]),
            ReadIntent::ContentRead,
        ),
        &cancel,
    );
    assert!(matches!(empty, Err(TargetAccessError::Range(_))));
    cancel.store(true, Ordering::Release);
    let error = full.read_to_end(&mut Vec::new()).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(matches!(
        error
            .get_ref()
            .unwrap()
            .downcast_ref::<source_kernel::source_runtime::range_view::RangeReadError>(),
        Some(source_kernel::source_runtime::range_view::RangeReadError::Cancelled)
    ));
    assert!(full.seek(SeekFrom::Start(0)).is_err());
    assert!(full.reserve_work(1).is_err());
    drop(full);
    drop(collection);
    workspace.close().unwrap();
}

#[test]
fn collection_truncation_keeps_unexpected_eof_error() {
    let workspace = workspace();
    let paths = native_members(workspace.path());
    let sources = paths
        .iter()
        .map(|path| LvmDiscoverySource::new(path, DataSourceKind::Raw))
        .collect::<Vec<_>>();
    let cancel = AtomicBool::new(false);
    let collection = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    let access = plan_target_access(
        request(
            AccessTarget::CollectionSource {
                collection: &collection,
                path: &paths[0],
                kind: &DataSourceKind::Raw,
            },
            "collection-source",
            Some(vec![ByteRange::new(0, 4096).unwrap()]),
            ReadIntent::ContentRead,
        ),
        &cancel,
    )
    .unwrap();
    let mut view = open_verified_view(access, &cancel).unwrap();
    let changed = std::fs::OpenOptions::new()
        .write(true)
        .open(&paths[0])
        .unwrap();
    changed.set_len(0).unwrap();
    let error = view.read(&mut [0; 1]).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::UnexpectedEof);
    drop(changed);
    drop(view);
    drop(collection);
    workspace.close().unwrap();
}

#[test]
fn access_error_conversion_keeps_native_md_and_io_variants() {
    use evidence_access::mdraid::MdRaidPlanError;
    assert!(matches!(
        MdRaidPlanError::from(TargetAccessError::MdRaid(
            MdRaidPlanError::SourceContentChanged
        )),
        MdRaidPlanError::SourceContentChanged
    ));
    let error = MdRaidPlanError::from(TargetAccessError::Io(std::io::Error::new(
        std::io::ErrorKind::UnexpectedEof,
        "native source truncated",
    )));
    assert!(
        matches!(error, MdRaidPlanError::Io(error) if error.kind() == std::io::ErrorKind::UnexpectedEof)
    );
    assert!(matches!(
        MdRaidPlanError::from(TargetAccessError::Cancelled),
        MdRaidPlanError::Cancelled
    ));
}
