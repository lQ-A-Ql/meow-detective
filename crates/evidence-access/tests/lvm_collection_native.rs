use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

use domain::DataSourceKind;
use evidence_access::datasource_service::{
    self, CollectionPlanError, ImageCollectionPlan, LvmDiscoverySource, PartitionStatus,
};
use sha2::{Digest, Sha256};

// Each case owns a full content scan and retained plans in the process-wide pool.
static COLLECTION_BUDGET: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn sources(root: &Path) -> Vec<LvmDiscoverySource> {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/lvm-native/shared-ext4");
    (0..2)
        .map(|index| {
            let path = root.join(format!("pv-{index}.raw"));
            std::io::copy(
                &mut flate2::read::GzDecoder::new(
                    File::open(fixture.join(format!("pv-{index}.raw.gz"))).unwrap(),
                ),
                &mut File::create(&path).unwrap(),
            )
            .unwrap();
            LvmDiscoverySource::new(path, DataSourceKind::Raw)
        })
        .collect()
}

#[test]
fn native_cross_pv_view_is_an_independent_logical_source_matching_the_kernel_digest() {
    let _budget = COLLECTION_BUDGET.lock().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut sources = sources(workspace.path());
    let cancel = AtomicBool::new(false);
    let plan = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    let owner = plan
        .member_probe(0, &sources[0].source_path, &sources[0].source_kind)
        .unwrap();
    let secondary = plan
        .member_probe(1, &sources[1].source_path, &sources[1].source_kind)
        .unwrap();
    assert!(owner.candidates.is_empty());
    assert!(secondary.candidates.is_empty());
    assert_eq!(secondary.partitions.len(), 1);
    assert_eq!(secondary.partitions[0].status, PartitionStatus::Expanded);
    assert_eq!(plan.logical_volume_candidates().len(), 1);
    let identity = plan.logical_volume_candidates()[0]
        .lvm_identity
        .as_ref()
        .unwrap();
    assert_eq!(identity.pv_sources.len(), 2);
    assert_ne!(
        identity.pv_sources[0].source_path,
        identity.pv_sources[1].source_path
    );
    assert_eq!(identity.pv_sources[0].offset, identity.pv_sources[1].offset);
    let pool = fs_lvm::LvmPool::discover(
        identity
            .pv_sources
            .iter()
            .map(|source| {
                datasource_service::open_evidence_reader(
                    Path::new(&source.source_path),
                    source.source_kind.as_ref().unwrap(),
                )
                .unwrap()
            })
            .collect(),
        identity
            .pv_sources
            .iter()
            .map(|source| source.offset)
            .collect(),
    )
    .unwrap();
    let index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.uuid == identity.lv_uuid)
        .unwrap();
    let mut reader = pool.open_volume_reader(index).unwrap();
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer).unwrap();
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    assert_eq!(
        hex::encode(digest.finalize()),
        "e8c4c41f356b5e202206ad532b467954e3fe3b5826de1f19f236c66f60849586"
    );
    sources.reverse();
    let reversed = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    assert_eq!(plan.content_fingerprint(), reversed.content_fingerprint());
    assert!(reversed
        .member_probe(0, &sources[0].source_path, &sources[0].source_kind)
        .unwrap()
        .candidates
        .is_empty());
    assert!(reversed
        .member_probe(1, &sources[1].source_path, &sources[1].source_kind)
        .unwrap()
        .candidates
        .is_empty());
    assert_eq!(
        reversed.logical_volume_candidates()[0].lvm_identity,
        plan.logical_volume_candidates()[0].lvm_identity
    );
}

#[test]
fn native_collection_rejects_duplicate_pv_and_invalidates_changed_members() {
    let _budget = COLLECTION_BUDGET.lock().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut sources = sources(workspace.path());
    let cancel = AtomicBool::new(false);
    let plan = ImageCollectionPlan::prepare(&sources, &cancel).unwrap();
    assert!(matches!(
        plan.member_probe(0, &sources[1].source_path, &DataSourceKind::Raw),
        Err(CollectionPlanError::MemberMismatch)
    ));
    let duplicate = workspace.path().join("copy.raw");
    std::fs::copy(&sources[0].source_path, &duplicate).unwrap();
    sources.push(LvmDiscoverySource::new(duplicate, DataSourceKind::Raw));
    assert!(matches!(
        ImageCollectionPlan::prepare(&sources, &cancel),
        Err(CollectionPlanError::DuplicateIdentity)
    ));
    File::options()
        .write(true)
        .open(&sources[1].source_path)
        .unwrap()
        .set_len(1)
        .unwrap();
    assert!(plan.validate_snapshot().is_err());
    assert!(plan
        .member_probe(0, &sources[0].source_path, &DataSourceKind::Raw)
        .is_err());
    assert!(matches!(
        ImageCollectionPlan::prepare(&sources, &AtomicBool::new(true)),
        Err(CollectionPlanError::Cancelled)
    ));
}

#[test]
fn collection_admission_propagates_thin_transaction_failure() {
    let _budget = COLLECTION_BUDGET.lock().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../testdata/fixtures/public-small/lvm-native/thin/pv-transaction-conflict.raw.gz",
    );
    let path = workspace.path().join("thin-conflict.raw");
    std::io::copy(
        &mut flate2::read::GzDecoder::new(File::open(fixture).unwrap()),
        &mut File::create(&path).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        ImageCollectionPlan::prepare(
            &[LvmDiscoverySource::new(path, DataSourceKind::Raw)],
            &AtomicBool::new(false)
        ),
        Err(CollectionPlanError::Lvm(
            fs_lvm::LvmError::ThinPoolTransactionMismatch {
                expected: 1,
                actual: 2
            }
        )),
    ));
}

#[test]
fn collection_content_detects_replacement_with_unchanged_length_and_timestamps() {
    let _budget = COLLECTION_BUDGET.lock().unwrap();
    use std::io::{Seek, SeekFrom, Write};
    let workspace = tempfile::tempdir().unwrap();
    let sources = sources(workspace.path());
    let cancel = AtomicBool::new(false);
    let plan = std::sync::Arc::new(ImageCollectionPlan::prepare(&sources, &cancel).unwrap());
    plan.verify_content(&cancel).unwrap();
    let factory = plan
        .logical_volume_factory(&plan.logical_volume_candidates()[0])
        .unwrap();
    let modified = sources[1]
        .source_path
        .metadata()
        .unwrap()
        .modified()
        .unwrap();
    let mut file = File::options()
        .read(true)
        .write(true)
        .open(&sources[1].source_path)
        .unwrap();
    let changed_offset = 2 * 1024 * 1024;
    file.seek(SeekFrom::Start(changed_offset)).unwrap();
    let mut byte = [0];
    file.read_exact(&mut byte).unwrap();
    file.seek(SeekFrom::Start(changed_offset)).unwrap();
    file.write_all(&[byte[0] ^ 0x80]).unwrap();
    file.sync_all().unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    drop(file);
    plan.validate_snapshot().unwrap();
    assert!(plan
        .validate_member_inputs(0, &sources[0].source_path, &DataSourceKind::Raw, &cancel)
        .is_ok());
    let verification = plan.verify_content(&cancel);
    assert!(
        matches!(
            verification,
            Err(CollectionPlanError::Content(
                evidence_access::source_runtime::content::ContentError::Changed
            ))
        ),
        "{verification:?}"
    );
    let mut direct = plan
        .verified_factory(&sources[1].source_path, &DataSourceKind::Raw, &cancel)
        .unwrap()
        .open_reader()
        .unwrap();
    direct.seek(SeekFrom::Start(changed_offset)).unwrap();
    assert!(direct.read_exact(&mut byte).is_err());
    assert!(factory.open_reader().is_err());
    assert!(plan
        .validate_member_inputs(0, &sources[0].source_path, &DataSourceKind::Raw, &cancel)
        .is_err());
    assert!(matches!(
        plan.verify_content(&AtomicBool::new(true)),
        Err(CollectionPlanError::Cancelled)
    ));
}

#[test]
fn independently_complete_cloned_vgs_keep_separate_content_and_physical_owners() {
    let _budget = COLLECTION_BUDGET.lock().unwrap();
    use std::io::{Seek, SeekFrom, Write};
    let workspace = tempfile::tempdir().unwrap();
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/lvm-native/shared-ext4");
    let mut members = Vec::new();
    for acquisition in ["node-a", "node-b"] {
        for index in 0..2 {
            let path = workspace.path().join(format!("{acquisition}-{index}.raw"));
            std::io::copy(
                &mut flate2::read::GzDecoder::new(
                    File::open(fixture.join(format!("pv-{index}.raw.gz"))).unwrap(),
                ),
                &mut File::create(&path).unwrap(),
            )
            .unwrap();
            members.push(path);
        }
    }
    let first = members[0].clone();
    let second = members[3].clone();
    let mut changed = File::options()
        .read(true)
        .write(true)
        .open(&second)
        .unwrap();
    changed.seek(SeekFrom::End(-1)).unwrap();
    let mut byte = [0];
    changed.read_exact(&mut byte).unwrap();
    changed.seek(SeekFrom::End(-1)).unwrap();
    changed.write_all(&[byte[0] ^ 0x80]).unwrap();
    drop(changed);
    let plan_a = ImageCollectionPlan::prepare(
        &members[..2]
            .iter()
            .map(|path| LvmDiscoverySource::new(path, DataSourceKind::Raw))
            .collect::<Vec<_>>(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let plan_b = ImageCollectionPlan::prepare(
        &members[2..]
            .iter()
            .map(|path| LvmDiscoverySource::new(path, DataSourceKind::Raw))
            .collect::<Vec<_>>(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_ne!(
        plan_a
            .source_content(&first, &DataSourceKind::Raw)
            .unwrap()
            .logical_sha256,
        plan_b
            .source_content(&second, &DataSourceKind::Raw)
            .unwrap()
            .logical_sha256
    );
    for (plan, acquisition, slice) in [
        (&plan_a, "node-a", &members[..2]),
        (&plan_b, "node-b", &members[2..]),
    ] {
        for (index, path) in slice.iter().enumerate() {
            let source = LvmDiscoverySource::new(path, DataSourceKind::Raw);
            let facts = plan
                .native_observations(&source.source_path, &source.source_kind)
                .unwrap();
            assert!(facts
                .iter()
                .any(|fact| matches!(fact.version, domain::structure::NativeVersion::LvmLabel)));
            let probe = plan
                .member_probe(index, &source.source_path, &source.source_kind)
                .unwrap();
            assert!(probe
                .partitions
                .iter()
                .any(|partition| partition.status == PartitionStatus::Expanded));
            assert!(probe
                .partitions
                .iter()
                .all(|partition| partition.lvm_identity.is_none()));
        }
        let logical = plan.logical_volume_candidates();
        assert_eq!(logical.len(), 1);
        let identity = logical[0].lvm_identity.as_ref().unwrap();
        assert_eq!(identity.pv_sources.len(), 2);
        assert!(identity.pv_sources.iter().all(|source| {
            Path::new(&source.source_path)
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(acquisition))
        }));
    }
    let identity_a = plan_a.logical_volume_candidates()[0]
        .lvm_identity
        .as_ref()
        .unwrap();
    let identity_b = plan_b.logical_volume_candidates()[0]
        .lvm_identity
        .as_ref()
        .unwrap();
    assert_eq!(identity_a.vg_uuid, identity_b.vg_uuid);
    assert_eq!(identity_a.lv_uuid, identity_b.lv_uuid);
    assert_ne!(
        plan_a.logical_volume_candidates()[0]
            .lvm_identity
            .as_ref()
            .unwrap()
            .pv_sources[0]
            .source_path,
        plan_b.logical_volume_candidates()[0]
            .lvm_identity
            .as_ref()
            .unwrap()
            .pv_sources[0]
            .source_path
    );
    let modified = second.metadata().unwrap().modified().unwrap();
    let mut changed = File::options()
        .read(true)
        .write(true)
        .open(&second)
        .unwrap();
    changed.seek(SeekFrom::End(-1)).unwrap();
    changed.read_exact(&mut byte).unwrap();
    changed.seek(SeekFrom::End(-1)).unwrap();
    changed.write_all(&[byte[0] ^ 0x40]).unwrap();
    changed.sync_all().unwrap();
    changed
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    drop(changed);
    plan_b.validate_snapshot().unwrap();
    plan_a
        .validate_member_inputs(
            0,
            &members[0],
            &DataSourceKind::Raw,
            &AtomicBool::new(false),
        )
        .unwrap();
    plan_a
        .validate_member_inputs(
            1,
            &members[1],
            &DataSourceKind::Raw,
            &AtomicBool::new(false),
        )
        .unwrap();
    plan_b
        .validate_member_inputs(
            0,
            &members[2],
            &DataSourceKind::Raw,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert!(plan_b
        .validate_member_inputs(1, &second, &DataSourceKind::Raw, &AtomicBool::new(false))
        .is_ok());
    assert!(plan_b.verify_content(&AtomicBool::new(false)).is_err());
    plan_a.verify_content(&AtomicBool::new(false)).unwrap();
}
