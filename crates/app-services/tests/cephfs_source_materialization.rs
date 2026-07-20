use std::{collections::BTreeMap, path::PathBuf};

use app_services::{
    analysis_service::run_source_analysis_extraction,
    case_service,
    ceph_reconstruction::{
        materialize_cephfs_source, CephFsDescriptor, CephFsDescriptorState,
        CephFsFilesystemPresenceRecord, CephFsMapProvenance, CephFsPoolBinding,
        CephFsPoolProvenance, CephFsPoolRole, CephFsPresenceAssessment, CephFsPresenceState,
        CephFsSourceError, CephFsSourceMaterializationRequest, CephFsSparseExtentProof,
    },
    file_service::{
        get_file_tree_for_case, open_preview_session_for_case, read_preview_bytes_for_source_case,
        read_preview_session_bytes_for_case, PreviewRuntimeRegistry,
    },
};
use ceph_wire::{
    assemble_cephfs_namespace, CephFsDentryKey, CephFsDentryKind, CephFsDentryProjection,
    CephFsDirfragBatch, CephFsDirfragIdentity, CephFsInodeKind, CephFsInodeProjection,
    CephFsMetadataMutationState, CephFsNamespaceAssemblyInput, CephFsNamespaceEntry,
    CephFsNamespaceEntryKind, CephFsNamespaceRecord, CEPH_NOSNAP,
};
use chrono::Utc;
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_cluster_repo::{DataSourceClusterRecord, DataSourceClusterRepo},
    datasource_repo::{DataSourceRepo, DataSourceStorage},
};

const CASE_ID: &str = "case-cephfs-materialization";
const CLUSTER_ID: &str = "cluster-cephfs-materialization";
const FILESYSTEM_ID: &str = "ceph-fs:cluster-a:1:17:7";
const FILE_ENTRY_ID: &str = "cephfs:0000000000000001:00000000:0000000000000002:hello.txt";
const FILE_CONTENT: &[u8] = b"CephFS bounded preview\n";

struct Fixture {
    root: tempfile::TempDir,
    case_conn: rusqlite::Connection,
    case_id: CaseId,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().expect("create case root");
    let case_conn = persistence_sqlite::open_or_create(&root.path().join("app.db"))
        .expect("open case database");
    persistence_sqlite::runner::run_all(&case_conn).expect("run case migrations");
    let case_id = CaseId(CASE_ID.to_string());
    CaseRepo::new(&case_conn)
        .create(&domain::CaseMeta {
            id: case_id.clone(),
            name: "CephFS materialization".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
        .expect("insert case");
    DataSourceClusterRepo::new(&case_conn)
        .insert_pending(&DataSourceClusterRecord {
            id: CLUSTER_ID.to_string(),
            case_id: case_id.clone(),
            name: "cluster-a".to_string(),
            root_path: root.path().display().to_string(),
            platform: "linux".to_string(),
            profile: Some("pve_cluster".to_string()),
            manifest_rel_path: "clusters/cluster-a/manifest.json".to_string(),
            import_state: "ready".to_string(),
            member_count: 3,
            ready_count: 3,
            failed_count: 0,
            last_error: None,
        })
        .expect("insert cluster");

    for (index, source_id) in ["osd-0", "osd-1", "osd-2"].into_iter().enumerate() {
        let id = DataSourceId(source_id.to_string());
        let source = DataSource {
            id: id.clone(),
            name: source_id.to_string(),
            kind: DataSourceKind::E01,
            source_path: PathBuf::from(format!("{source_id}.E01")),
            imported_at: Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(
            source_id,
            Some("linux"),
            Some("cluster_member".to_string()),
        );
        storage.import_state = "ready".to_string();
        DataSourceRepo::new(&case_conn)
            .insert_with_storage(&case_id, &source, &storage)
            .expect("register cluster member");
        DataSourceRepo::new(&case_conn)
            .update_cluster_membership(&id, CLUSTER_ID, index as u32, 3)
            .expect("bind cluster member");
        drop(app_services::source_db::open_source_db(root.path(), &id).expect("create source db"));
    }

    Fixture {
        root,
        case_conn,
        case_id,
    }
}

fn descriptor() -> CephFsDescriptor {
    let captured_at = chrono::DateTime::parse_from_rfc3339("2026-07-19T00:00:00Z")
        .expect("parse stable capture time")
        .with_timezone(&Utc);
    let bindings = || {
        ["osd-0", "osd-1", "osd-2"]
            .into_iter()
            .enumerate()
            .map(|(index, source)| CephFsPoolProvenance {
                source_identity: source.to_string(),
                inventory_identity: format!("inventory-{index}"),
            })
            .collect()
    };
    CephFsDescriptor {
        identity: FILESYSTEM_ID.to_string(),
        cluster_identity: "cluster-a".to_string(),
        filesystem_id: 1,
        name: "cephfs-a".to_string(),
        fsmap_epoch: 17,
        mdsmap_epoch: 19,
        state: CephFsDescriptorState::Present,
        metadata_pool: CephFsPoolBinding {
            pool_id: 7,
            role: CephFsPoolRole::Metadata,
            provenance: bindings(),
        },
        data_pools: vec![CephFsPoolBinding {
            pool_id: 8,
            role: CephFsPoolRole::Data { ordinal: 0 },
            provenance: bindings(),
        }],
        rank_bindings: Vec::new(),
        daemons: Vec::new(),
        provenance: ["osd-0", "osd-1", "osd-2"]
            .into_iter()
            .enumerate()
            .map(|(index, source)| CephFsMapProvenance {
                source_identity: source.to_string(),
                inventory_identity: format!("inventory-{index}"),
                captured_at,
                raw_fsmap_sha256: "a".repeat(64),
                raw_mdsmap_sha256: "b".repeat(64),
            })
            .collect(),
    }
}

fn namespace(complete: bool) -> ceph_wire::CephFsNamespaceGraph {
    let empty_layout =
        ceph_wire::CephFsFileLayout::new(0, 0, 0, -1, "").expect("create empty CephFS layout");
    let root = ceph_wire::CephFsNamespaceEntry {
        entry_id: "cephfs:root:0000000000000001".to_string(),
        parent_entry_id: None,
        parent_inode: 0,
        inode: 1,
        fragment: 0,
        name: "/".to_string(),
        path: "/".to_string(),
        kind: CephFsNamespaceEntryKind::Directory,
        mode: Some(0o040755),
        uid: Some(0),
        gid: Some(0),
        nlink: Some(2),
        size: Some(0),
        layout: Some(empty_layout.clone()),
        encoded_version: Some(1),
        remaining_inode_bytes: Some(0),
        alternate_name: String::new(),
    };
    let file = ceph_wire::CephFsNamespaceEntry {
        entry_id: FILE_ENTRY_ID.to_string(),
        parent_entry_id: Some(root.entry_id.clone()),
        parent_inode: 1,
        inode: 2,
        fragment: 0,
        name: "hello.txt".to_string(),
        path: "/hello.txt".to_string(),
        kind: CephFsNamespaceEntryKind::File,
        mode: Some(0o100644),
        uid: Some(1000),
        gid: Some(1000),
        nlink: Some(1),
        size: Some(FILE_CONTENT.len() as u64),
        layout: Some(empty_layout),
        encoded_version: Some(1),
        remaining_inode_bytes: Some(0),
        alternate_name: String::new(),
    };
    ceph_wire::CephFsNamespaceGraph {
        filesystem_root_inode: 1,
        root,
        entries: vec![file],
        diagnostics: Vec::new(),
        complete,
    }
}

fn materialize(
    fixture: &Fixture,
    complete: bool,
) -> app_services::ceph_reconstruction::MaterializedCephFsSource {
    materialize_result(fixture, complete).expect("materialize CephFS source")
}

fn materialize_result(
    fixture: &Fixture,
    complete: bool,
) -> Result<
    app_services::ceph_reconstruction::MaterializedCephFsSource,
    app_services::ceph_reconstruction::CephFsSourceError,
> {
    let graph = namespace(complete);
    materialize_result_with_namespace(fixture, &graph)
}

fn materialize_result_with_namespace(
    fixture: &Fixture,
    graph: &ceph_wire::CephFsNamespaceGraph,
) -> Result<
    app_services::ceph_reconstruction::MaterializedCephFsSource,
    app_services::ceph_reconstruction::CephFsSourceError,
> {
    let presence = present_assessment();
    materialize_result_with_presence(fixture, graph, &presence)
}

fn present_assessment() -> CephFsPresenceAssessment {
    CephFsPresenceAssessment {
        state: CephFsPresenceState::Present,
        source_count: 3,
        source_ids: vec![
            "osd-0".to_string(),
            "osd-1".to_string(),
            "osd-2".to_string(),
        ],
        cluster_identity: Some("cluster-a".to_string()),
        filesystem_count: 1,
        filesystems: vec![CephFsFilesystemPresenceRecord {
            filesystem_id: 1,
            metadata_pool_id: 7,
            data_pool_ids: vec![8],
        }],
        fsmap_epoch: Some(17),
        mdsmap_epoch: Some(19),
        diagnostics: Vec::new(),
    }
}

fn materialize_result_with_presence(
    fixture: &Fixture,
    graph: &ceph_wire::CephFsNamespaceGraph,
    presence: &CephFsPresenceAssessment,
) -> Result<
    app_services::ceph_reconstruction::MaterializedCephFsSource,
    app_services::ceph_reconstruction::CephFsSourceError,
> {
    let mut inline = BTreeMap::new();
    inline.insert(2, FILE_CONTENT.to_vec());
    materialize_result_with_content(fixture, graph, presence, &inline, &BTreeMap::new())
}

fn materialize_result_with_content(
    fixture: &Fixture,
    graph: &ceph_wire::CephFsNamespaceGraph,
    presence: &CephFsPresenceAssessment,
    inline: &BTreeMap<u64, Vec<u8>>,
    sparse_extents: &BTreeMap<u64, Vec<CephFsSparseExtentProof>>,
) -> Result<
    app_services::ceph_reconstruction::MaterializedCephFsSource,
    app_services::ceph_reconstruction::CephFsSourceError,
> {
    let assembly = assembly_for(graph);
    let descriptor = descriptor();
    materialize_cephfs_source(CephFsSourceMaterializationRequest {
        case_conn: &fixture.case_conn,
        case_root: fixture.root.path(),
        case_id: &fixture.case_id,
        cluster_id: CLUSTER_ID,
        presence,
        descriptor: &descriptor,
        namespace_assembly_input: &assembly,
        namespace_input_sha256: &"c".repeat(64),
        journal_boundary_sha256: None,
        inline_data_by_inode: inline,
        sparse_extents_by_inode: sparse_extents,
        expected_replica_count: 3,
    })
}

fn assembly_for(graph: &ceph_wire::CephFsNamespaceGraph) -> CephFsNamespaceAssemblyInput {
    let root = CephFsDirfragIdentity::new(graph.root.inode, graph.root.fragment)
        .expect("create root dirfrag identity");
    let records = graph
        .entries
        .iter()
        .map(namespace_record)
        .collect::<Vec<_>>();
    CephFsNamespaceAssemblyInput {
        root_inode: inode_projection(&graph.root),
        expected_dirfrags: vec![root.clone()],
        batches: vec![CephFsDirfragBatch {
            identity: root,
            records,
            complete: graph.complete,
            parent_proof: None,
        }],
        mutation_state: CephFsMetadataMutationState::Complete,
    }
}

fn namespace_record(entry: &CephFsNamespaceEntry) -> CephFsNamespaceRecord {
    let kind = if entry.kind == CephFsNamespaceEntryKind::Remote {
        CephFsDentryKind::Remote { d_type: 0 }
    } else {
        CephFsDentryKind::Primary
    };
    CephFsNamespaceRecord {
        parent: CephFsDirfragIdentity::new(entry.parent_inode, entry.fragment)
            .expect("create dentry parent identity"),
        dentry: CephFsDentryProjection {
            key: CephFsDentryKey {
                name: entry.name.clone(),
                snap_id: CEPH_NOSNAP,
            },
            first_snap: CEPH_NOSNAP,
            kind,
            child_inode: entry.inode,
            alternate_name: entry.alternate_name.clone(),
            inode: (entry.kind != CephFsNamespaceEntryKind::Remote)
                .then(|| inode_projection(entry)),
        },
    }
}

fn inode_projection(entry: &CephFsNamespaceEntry) -> CephFsInodeProjection {
    CephFsInodeProjection {
        ino: entry.inode,
        mode: entry.mode.expect("fixture inode mode"),
        uid: entry.uid.expect("fixture inode uid"),
        gid: entry.gid.expect("fixture inode gid"),
        nlink: entry.nlink.expect("fixture inode link count"),
        size: entry.size.expect("fixture inode size"),
        kind: match entry.kind {
            CephFsNamespaceEntryKind::File => CephFsInodeKind::File,
            CephFsNamespaceEntryKind::Directory => CephFsInodeKind::Directory,
            CephFsNamespaceEntryKind::Symlink => CephFsInodeKind::Symlink,
            CephFsNamespaceEntryKind::Remote | CephFsNamespaceEntryKind::Other => {
                CephFsInodeKind::Other
            }
        },
        layout: entry.layout.clone().expect("fixture inode layout"),
        encoded_version: entry.encoded_version.expect("fixture inode version"),
        remaining_inode_bytes: entry
            .remaining_inode_bytes
            .expect("fixture remaining inode bytes"),
    }
}

fn registered_cephfs_source_id(fixture: &Fixture) -> DataSourceId {
    DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .expect("list registered sources")
        .into_iter()
        .find(|source| source.kind == DataSourceKind::CephFs)
        .expect("CephFS source exists")
        .id
}

#[test]
fn complete_cephfs_source_publishes_tree_and_bounded_preview() {
    let fixture = fixture();
    let expected_assembly_sha256 = assemble_cephfs_namespace(assembly_for(&namespace(true)))
        .expect("assemble expected namespace")
        .assembly_sha256()
        .to_string();
    let result = materialize(&fixture, true);
    assert!(result.published);
    assert_eq!(result.file_count, 2);

    let source_id = result.data_source.id.0.clone();
    let storage = DataSourceRepo::new(&fixture.case_conn)
        .find_storage(&result.data_source.id)
        .expect("read CephFS storage")
        .expect("CephFS storage exists");
    assert_eq!(storage.import_state, "ready");
    assert_eq!(storage.platform, "linux");
    assert_eq!(storage.profile.as_deref(), Some("ceph_fs"));
    assert_eq!(
        result.capability,
        app_services::ceph_reconstruction::CephFsSourceCapability::BoundedPreview
    );

    let source_path =
        app_services::source_db::source_db_path(fixture.root.path(), &result.data_source.id);
    let source_conn = persistence_sqlite::open_existing_source_read_only(&source_path)
        .expect("open published CephFS source database");
    let assembly = persistence_sqlite::repositories::ceph_fs_namespace_assembly_repo::
        CephFsNamespaceAssemblyRepo::new(&source_conn)
        .find(FILESYSTEM_ID, &result.data_source.id.0)
        .expect("read namespace assembly")
        .expect("namespace assembly exists");
    assert!(assembly.complete);
    assert!(!assembly.frozen);
    assert_eq!(assembly.assembly_sha256, expected_assembly_sha256);
    let capability =
        persistence_sqlite::repositories::ceph_fs_capability_repo::CephFsSourceCapabilityRepo::new(
            &source_conn,
        )
        .find(FILESYSTEM_ID, &result.data_source.id.0)
        .expect("read source capability")
        .expect("source capability exists");
    assert_eq!(
        capability.capability,
        persistence_sqlite::repositories::ceph_fs_capability_repo::CephFsSourceCapability::
            BoundedPreview
    );

    let tree = get_file_tree_for_case(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        false,
    )
    .expect("read published CephFS tree");
    assert_eq!(tree.len(), 1);
    assert!(tree[0].id.starts_with(&format!("ds:{source_id}:")));

    let file_id = format!("ds:{source_id}:{FILE_ENTRY_ID}");
    assert_eq!(
        read_preview_bytes_for_source_case(
            &fixture.case_conn,
            fixture.root.path(),
            &fixture.case_id,
            &file_id,
            0,
            FILE_CONTENT.len() as u32,
        )
        .expect("read inline CephFS bytes"),
        FILE_CONTENT
    );

    let registry = PreviewRuntimeRegistry::default();
    let handle = open_preview_session_for_case(
        &registry,
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        &file_id,
    )
    .expect("open CephFS preview session");
    assert!(handle.handle_id.starts_with("preview:"));
    assert_eq!(
        registry
            .stats()
            .expect("read preview registry statistics")
            .session_count,
        1
    );
    assert_eq!(
        read_preview_session_bytes_for_case(
            &registry,
            &fixture.case_conn,
            fixture.root.path(),
            &fixture.case_id,
            &handle.handle_id,
            0,
            FILE_CONTENT.len() as u32,
        )
        .expect("read CephFS preview session"),
        FILE_CONTENT
    );
    let analysis_error = run_source_analysis_extraction(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        &result.data_source.id,
        &[],
    )
    .expect_err("CephFS must not enter host artifact extraction");
    assert!(analysis_error
        .to_string()
        .contains("does not run host-platform artifact extraction"));
}

#[test]
fn metadata_browseable_source_keeps_tree_but_rejects_file_preview() {
    let fixture = fixture();
    let mut graph = namespace(true);
    graph.entries[0].layout = Some(
        ceph_wire::CephFsFileLayout::new(65_536, 1, 65_536, 8, "")
            .expect("create object-backed CephFS layout"),
    );
    let presence = present_assessment();
    let result = materialize_result_with_content(
        &fixture,
        &graph,
        &presence,
        &BTreeMap::new(),
        &BTreeMap::new(),
    )
    .expect("materialize metadata-browseable source");

    assert_eq!(
        result.capability,
        app_services::ceph_reconstruction::CephFsSourceCapability::MetadataBrowseable
    );
    let tree = get_file_tree_for_case(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        false,
    )
    .expect("read metadata-browseable tree");
    assert_eq!(tree.len(), 1);
    let file_id = format!("ds:{}:{FILE_ENTRY_ID}", result.data_source.id.0);
    let error = read_preview_bytes_for_source_case(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        &file_id,
        0,
        FILE_CONTENT.len() as u32,
    )
    .expect_err("metadata-browseable source must not expose file bytes");
    assert!(error.to_string().contains("bounded-preview"));
}

#[test]
fn sparse_file_round_trips_evidence_proof_and_zero_fills_preview() {
    const SPARSE_SIZE: u64 = 4096;

    let fixture = fixture();
    let mut graph = namespace(true);
    graph.entries[0].size = Some(SPARSE_SIZE);
    let presence = present_assessment();
    let proof = CephFsSparseExtentProof::from_evidence(2, 0, SPARSE_SIZE, "c".repeat(64))
        .expect("build sparse evidence proof");
    let sparse = BTreeMap::from([(2, vec![proof.clone()])]);
    let result =
        materialize_result_with_content(&fixture, &graph, &presence, &BTreeMap::new(), &sparse)
            .expect("materialize sparse CephFS source");

    let file_id = format!("ds:{}:{FILE_ENTRY_ID}", result.data_source.id.0);
    let bytes = read_preview_bytes_for_source_case(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        &file_id,
        128,
        512,
    )
    .expect("read sparse preview range");
    assert_eq!(bytes, vec![0; 512]);

    let source_path =
        app_services::source_db::source_db_path(fixture.root.path(), &result.data_source.id);
    let source_conn = persistence_sqlite::open_existing_source_read_only(&source_path)
        .expect("open sparse source database");
    let locator =
        persistence_sqlite::repositories::ceph_fs_namespace_repo::CephFsNamespaceRepo::new(
            &source_conn,
        )
        .find_file_locator(&result.data_source.id.0, FILE_ENTRY_ID)
        .expect("read sparse locator")
        .expect("sparse locator exists");
    assert_eq!(locator.sparse_extents.len(), 1);
    assert_eq!(locator.sparse_extents[0].evidence_sha256, "c".repeat(64));
    assert_eq!(locator.sparse_extents[0].proof_sha256, proof.proof_sha256);
}

#[test]
fn inline_and_sparse_backing_cannot_publish_a_bounded_preview() {
    let fixture = fixture();
    let graph = namespace(true);
    let proof =
        CephFsSparseExtentProof::from_evidence(2, 0, FILE_CONTENT.len() as u64, "c".repeat(64))
            .expect("build conflicting sparse proof");
    let error = materialize_result_with_content(
        &fixture,
        &graph,
        &present_assessment(),
        &BTreeMap::from([(2, FILE_CONTENT.to_vec())]),
        &BTreeMap::from([(2, vec![proof])]),
    )
    .expect_err("mutually exclusive content backing must fail");
    assert!(matches!(error, CephFsSourceError::InvalidInput(_)));
    assert!(DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .expect("list sources after backing rejection")
        .iter()
        .all(|source| source.kind != DataSourceKind::CephFs));
}

#[test]
fn repeated_materialization_returns_the_registered_source_metadata() {
    let fixture = fixture();
    let first = materialize(&fixture, true);
    let second = materialize(&fixture, true);

    assert_eq!(second.data_source.id, first.data_source.id);
    assert_eq!(
        second.data_source.imported_at,
        first.data_source.imported_at
    );
    assert_eq!(second.catalog_digest, first.catalog_digest);
}

#[test]
fn ready_materialization_rejects_namespace_assembly_tampering() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let source_path =
        app_services::source_db::source_db_path(fixture.root.path(), &result.data_source.id);
    let source_conn = persistence_sqlite::open_or_create_source(&source_path)
        .expect("open source database for assembly tampering");
    source_conn
        .execute(
            "UPDATE ceph_fs_namespace_assemblies
             SET assembly_sha256 = ?1
             WHERE data_source_id = ?2",
            rusqlite::params!["e".repeat(64), result.data_source.id.0],
        )
        .expect("tamper assembly digest");
    drop(source_conn);

    let error = materialize_result(&fixture, true)
        .expect_err("tampered assembly must not be reported as ready");
    assert!(matches!(error, CephFsSourceError::StalePublication));
}

#[test]
fn ready_materialization_rejects_capability_downgrade() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let source_path =
        app_services::source_db::source_db_path(fixture.root.path(), &result.data_source.id);
    let source_conn = persistence_sqlite::open_or_create_source(&source_path)
        .expect("open source database for capability tampering");
    source_conn
        .execute(
            "UPDATE ceph_fs_source_capabilities
             SET capability = 'metadata-browseable'
             WHERE data_source_id = ?1",
            [result.data_source.id.0],
        )
        .expect("downgrade capability");
    drop(source_conn);

    let error = materialize_result(&fixture, true)
        .expect_err("downgraded capability must not be reported as ready");
    assert!(matches!(error, CephFsSourceError::StalePublication));
}

#[test]
fn ready_materialization_rejects_namespace_row_tampering() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let source_path =
        app_services::source_db::source_db_path(fixture.root.path(), &result.data_source.id);
    let source_conn = persistence_sqlite::open_or_create_source(&source_path)
        .expect("open published source database for tamper fixture");
    source_conn
        .execute(
            "UPDATE ceph_fs_dentries
             SET parent_inode = 99
             WHERE entry_id = ?1",
            [FILE_ENTRY_ID],
        )
        .expect("tamper namespace row");
    drop(source_conn);

    let error = materialize_result(&fixture, true)
        .expect_err("tampered namespace must not be reported as ready");
    assert!(matches!(error, CephFsSourceError::Namespace(_)));
}

#[test]
fn ready_materialization_rejects_file_catalog_tampering() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let source_path =
        app_services::source_db::source_db_path(fixture.root.path(), &result.data_source.id);
    let source_conn = persistence_sqlite::open_or_create_source(&source_path)
        .expect("open published source database for catalog tamper fixture");
    source_conn
        .execute(
            "UPDATE file_entries
             SET parent_id = NULL
             WHERE id = ?1",
            [FILE_ENTRY_ID],
        )
        .expect("tamper file catalog row");
    drop(source_conn);

    let error = materialize_result(&fixture, true)
        .expect_err("tampered file catalog must not be reported as ready");
    assert!(matches!(error, CephFsSourceError::Namespace(_)));
}

#[test]
fn incomplete_cephfs_namespace_is_retained_without_file_rows_or_ready_access() {
    let fixture = fixture();
    let result = materialize(&fixture, false);
    assert!(!result.published);
    assert_eq!(
        result.capability,
        app_services::ceph_reconstruction::CephFsSourceCapability::MetadataOnly
    );

    let storage = DataSourceRepo::new(&fixture.case_conn)
        .find_storage(&result.data_source.id)
        .expect("read incomplete storage")
        .expect("incomplete storage exists");
    assert_eq!(storage.import_state, "failed");
    let source_path =
        app_services::source_db::source_db_path(fixture.root.path(), &result.data_source.id);
    assert!(source_path.is_file());
    let source_conn = persistence_sqlite::open_existing_source_read_only(&source_path)
        .expect("open retained incomplete source database");
    let file_count: i64 = source_conn
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row.get(0))
        .expect("count incomplete file rows");
    assert_eq!(file_count, 0);
    let assembly = persistence_sqlite::repositories::ceph_fs_namespace_assembly_repo::
        CephFsNamespaceAssemblyRepo::new(&source_conn)
        .find(FILESYSTEM_ID, &result.data_source.id.0)
        .expect("read frozen assembly")
        .expect("frozen assembly exists");
    assert!(!assembly.complete);
    assert!(assembly.frozen);
    assert_ne!(assembly.freeze_reasons_json, "[]");
    let capability =
        persistence_sqlite::repositories::ceph_fs_capability_repo::CephFsSourceCapabilityRepo::new(
            &source_conn,
        )
        .find(FILESYSTEM_ID, &result.data_source.id.0)
        .expect("read metadata-only capability")
        .expect("metadata-only capability exists");
    assert_eq!(
        capability.capability,
        persistence_sqlite::repositories::ceph_fs_capability_repo::CephFsSourceCapability::
            MetadataOnly
    );
    assert!(app_services::source_db::open_ready_source_by_id(
        &fixture.case_conn,
        fixture.root.path(),
        &fixture.case_id,
        &result.data_source.id,
    )
    .is_err());
}

#[test]
fn retained_incomplete_source_requires_delete_before_reimport() {
    let fixture = fixture();
    let first = materialize(&fixture, false);
    let source_id = first.data_source.id.clone();

    let retry = materialize_result(&fixture, false)
        .expect_err("retained incomplete source must not be silently replaced");
    assert!(matches!(retry, CephFsSourceError::RetainedIncompleteSource));

    std::fs::create_dir_all(fixture.root.path().join("cache")).expect("create deletion cache");
    case_service::delete_data_source_in(&fixture.case_conn, fixture.root.path(), &source_id.0)
        .expect("delete retained incomplete source");
    let rebuilt = materialize(&fixture, true);
    assert!(rebuilt.published);
    assert_eq!(rebuilt.file_count, 2);
}

#[test]
fn published_source_is_recovered_after_case_state_commit_failure() {
    let fixture = fixture();
    fixture
        .case_conn
        .execute_batch(
            "CREATE TRIGGER fail_cephfs_ready_update
             BEFORE UPDATE OF import_state ON data_sources
             WHEN NEW.kind = 'ceph_fs' AND NEW.import_state = 'ready'
             BEGIN
                 SELECT RAISE(ABORT, 'injected CephFS ready-state failure');
             END;",
        )
        .expect("install failure trigger");

    let first_error = materialize_result(&fixture, true).expect_err("first publish must fail");
    assert!(
        first_error
            .to_string()
            .contains("injected CephFS ready-state failure")
            || first_error.to_string().contains("constraint failed"),
        "unexpected first publish error: {first_error}"
    );
    let source_id = registered_cephfs_source_id(&fixture);
    let source_path = app_services::source_db::source_db_path(fixture.root.path(), &source_id);
    assert!(
        source_path.is_file(),
        "sealed source DB must survive for recovery"
    );

    fixture
        .case_conn
        .execute_batch("DROP TRIGGER fail_cephfs_ready_update")
        .expect("remove failure trigger");
    let recovered = materialize(&fixture, true);
    assert!(recovered.published);
    assert_eq!(recovered.file_count, 2);
}

#[test]
fn stale_catalog_publication_is_not_reported_as_incomplete_namespace() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let source_id = result.data_source.id;
    fixture
        .case_conn
        .execute(
            "UPDATE data_source_catalog_publications
             SET catalog_digest = ?1
             WHERE data_source_id = ?2",
            rusqlite::params!["d".repeat(64), source_id.0],
        )
        .expect("invalidate catalog publication");

    let error = materialize_result(&fixture, true).expect_err("stale publication must fail");
    assert!(matches!(error, CephFsSourceError::StalePublication));
}

#[test]
fn ready_summary_requires_a_finalized_source_database() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let source_path =
        app_services::source_db::source_db_path(fixture.root.path(), &result.data_source.id);
    let source_conn = persistence_sqlite::open_or_create_source(&source_path)
        .expect("open published source database");
    source_conn
        .execute(
            "DELETE FROM source_meta WHERE key = 'source.build.finalized'",
            [],
        )
        .expect("remove finalized marker");
    drop(source_conn);

    let error = materialize_result(&fixture, true).expect_err("unsealed source must fail");
    assert!(matches!(error, CephFsSourceError::Database(_)));
    assert!(error.to_string().contains("not sealed"));
}

#[test]
fn deleting_cephfs_source_removes_only_derived_storage_and_lineage() {
    let fixture = fixture();
    let result = materialize(&fixture, true);
    let source_id = result.data_source.id;
    std::fs::create_dir_all(fixture.root.path().join("cache")).expect("create deletion cache");

    case_service::delete_data_source_in(&fixture.case_conn, fixture.root.path(), &source_id.0)
        .expect("delete CephFS source");

    assert!(!app_services::source_db::source_db_path(fixture.root.path(), &source_id).exists());
    assert!(DataSourceRepo::new(&fixture.case_conn)
        .find_storage(&source_id)
        .expect("query deleted storage")
        .is_none());
    assert!(
        persistence_sqlite::repositories::ceph_fs_lineage_repo::CephFsDerivedLineageRepo::new(
            &fixture.case_conn
        )
        .find_by_data_source(&source_id.0)
        .expect("query deleted lineage")
        .is_none()
    );
    let remaining_members = DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .expect("list remaining sources");
    assert_eq!(remaining_members.len(), 3);
    assert!(remaining_members
        .iter()
        .all(|source| source.kind == DataSourceKind::E01));
}

#[test]
fn invalid_cephfs_parent_reference_is_rejected_before_registration() {
    let fixture = fixture();
    let mut graph = namespace(true);
    graph.entries[0].parent_entry_id = Some("cephfs:missing-parent".to_string());
    graph.entries[0].parent_inode = 99;

    let error = materialize_result_with_namespace(&fixture, &graph)
        .expect_err("invalid parent reference must fail");
    assert!(matches!(error, CephFsSourceError::NamespaceAssembly(_)));
    assert!(DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .expect("list sources after rejected graph")
        .iter()
        .all(|source| source.kind != DataSourceKind::CephFs));
}

#[test]
fn indeterminate_presence_cannot_create_a_cephfs_source() {
    let fixture = fixture();
    let presence = CephFsPresenceAssessment {
        state: CephFsPresenceState::Indeterminate,
        source_count: 3,
        source_ids: Vec::new(),
        cluster_identity: None,
        filesystem_count: 0,
        filesystems: Vec::new(),
        fsmap_epoch: None,
        mdsmap_epoch: None,
        diagnostics: Vec::new(),
    };

    let error = materialize_result_with_presence(&fixture, &namespace(true), &presence)
        .expect_err("indeterminate presence must not materialize");
    assert!(matches!(error, CephFsSourceError::PresenceNotProven(_)));
    assert!(DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .expect("list sources after presence gate")
        .iter()
        .all(|source| source.kind != DataSourceKind::CephFs));
}

#[test]
fn presence_identity_and_pool_mismatch_cannot_create_a_cephfs_source() {
    let fixture = fixture();
    let mut presence = present_assessment();
    presence.cluster_identity = Some("other-cluster".to_string());
    let cluster_error = materialize_result_with_presence(&fixture, &namespace(true), &presence)
        .expect_err("cluster identity mismatch must fail");
    assert!(matches!(
        cluster_error,
        CephFsSourceError::PresenceNotProven(_)
    ));

    presence.cluster_identity = Some("cluster-a".to_string());
    presence.filesystems[0].metadata_pool_id = 99;
    let pool_error = materialize_result_with_presence(&fixture, &namespace(true), &presence)
        .expect_err("pool binding mismatch must fail");
    assert!(matches!(
        pool_error,
        CephFsSourceError::PresenceNotProven(_)
    ));
    assert!(DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .expect("list sources after identity gate")
        .iter()
        .all(|source| source.kind != DataSourceKind::CephFs));
}

#[test]
fn presence_source_provenance_mismatch_cannot_create_a_cephfs_source() {
    let fixture = fixture();
    let mut presence = present_assessment();
    presence.source_ids[0] = "foreign-osd".to_string();

    let error = materialize_result_with_presence(&fixture, &namespace(true), &presence)
        .expect_err("descriptor provenance mismatch must fail");
    assert!(matches!(error, CephFsSourceError::PresenceNotProven(_)));
    assert!(DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .expect("list sources after provenance gate")
        .iter()
        .all(|source| source.kind != DataSourceKind::CephFs));
}

#[test]
fn invalid_cephfs_path_and_duplicate_dentry_are_rejected() {
    let fixture = fixture();
    let mut invalid_path = namespace(true);
    invalid_path.entries[0].name = "../not-derived".to_string();
    let path_error = materialize_result_with_namespace(&fixture, &invalid_path)
        .expect_err("invalid path must fail");
    assert!(matches!(
        path_error,
        CephFsSourceError::NamespaceAssembly(_)
    ));

    let mut duplicate = namespace(true);
    let mut duplicate_entry = duplicate.entries[0].clone();
    duplicate_entry.entry_id = "cephfs:duplicate-entry".to_string();
    duplicate.entries.push(duplicate_entry);
    let duplicate_error = materialize_result_with_namespace(&fixture, &duplicate)
        .expect_err("duplicate dentry must fail");
    assert!(matches!(
        duplicate_error,
        CephFsSourceError::NamespaceAssembly(_)
    ));
}

#[test]
fn closed_namespace_rejects_an_inconsistent_file_link_count() {
    let fixture = fixture();
    let mut graph = namespace(true);
    graph.entries[0].nlink = Some(2);

    let error = materialize_result_with_namespace(&fixture, &graph)
        .expect_err("closed namespace link count mismatch must fail");
    assert!(matches!(error, CephFsSourceError::InvalidInput(_)));
    assert!(DataSourceRepo::new(&fixture.case_conn)
        .find_by_case(&fixture.case_id)
        .expect("list sources after link-count rejection")
        .iter()
        .all(|source| source.kind != DataSourceKind::CephFs));
}
