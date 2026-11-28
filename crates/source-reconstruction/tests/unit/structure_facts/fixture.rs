use std::path::PathBuf;

use chrono::Utc;
use domain::{
    structure::{AcquisitionId, SourceRevisionContent, StructureGraph},
    CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance, EntryType, FileEntry,
    FileEntryId,
};
use persistence_source::repositories::{artifact_repo::ArtifactRepo, file_repo::FileRepo};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_revision_repo::SourceRevisionRepo,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

crate::native_source_test_support!();

pub struct CaseFixture {
    pub root: tempfile::TempDir,
    pub connection: rusqlite::Connection,
    pub case_id: CaseId,
}

impl CaseFixture {
    pub fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("artifacts/objects")).unwrap();
        let connection = persistence_sqlite::open_or_create(&root.path().join("app.db")).unwrap();
        persistence_sqlite::runner::run_all(&connection).unwrap();
        let case_id = CaseId("s04-acceptance".into());
        CaseRepo::new(&connection)
            .create(&domain::CaseMeta {
                id: case_id.clone(),
                name: "S04 acceptance".into(),
                number: None,
                examiner: None,
                notes: None,
                created_at: Utc::now(),
                updated_at: Utc::now(),
            })
            .unwrap();
        Self {
            root,
            connection,
            case_id,
        }
    }

    pub fn source(&self, source_id: &str, files: &[(&str, &[u8])]) {
        let input_root = self.root.path().join("inputs").join(source_id);
        std::fs::create_dir_all(&input_root).unwrap();
        let source = DataSource {
            id: DataSourceId(source_id.into()),
            name: source_id.into(),
            kind: DataSourceKind::LogicalDirectory,
            source_path: input_root.clone(),
            imported_at: Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(source_id, Some("linux"), None);
        storage.import_state = "ready".into();
        DataSourceRepo::new(&self.connection)
            .insert_with_storage(&self.case_id, &source, &storage)
            .unwrap();
        let conn = source_kernel::source_db::open_source_db(self.root.path(), &source.id).unwrap();
        DataSourceRepo::new(&conn)
            .upsert_source_local_metadata(&self.case_id, &source)
            .unwrap();
        let mut identity = Sha256::new();
        for (index, (path, bytes)) in files.iter().enumerate() {
            let actual_path = input_root.join(path);
            std::fs::create_dir_all(actual_path.parent().unwrap()).unwrap();
            std::fs::write(&actual_path, bytes).unwrap();
            identity.update((*path).as_bytes());
            identity.update(bytes);
            let entry = file_entry(&source.id, index, path, bytes);
            FileRepo::new(&conn)
                .insert_batch(std::slice::from_ref(&entry))
                .unwrap();
            if path.contains("etc/pve/") {
                let candidate = analysis_core::analysis_service::EvidenceCandidate {
                    file_id: entry.id.clone(),
                    data_source_id: source_id.into(),
                    partition_index: None,
                    path: entry.path.clone(),
                    size: bytes.len() as u64,
                    encrypted: false,
                    content_identity: hex::encode(Sha256::digest(bytes)),
                    companions: Vec::new(),
                    evidence_kind: "pve_config".into(),
                    parser: "linux.pve_config".into(),
                    category: "LinuxArtifacts".into(),
                    modified_at: None,
                };
                let extracted =
                    analysis_core::analysis_service::extract_linux_candidate(&candidate, bytes);
                assert!(!extracted.artifacts.is_empty());
                ArtifactRepo::new(&conn)
                    .insert_batch(&extracted.artifacts, &self.case_id.0, source_id)
                    .unwrap();
            }
        }
        let digest = hex::encode(identity.finalize());
        SourceRevisionRepo::new(&self.connection)
            .bind(
                &self.case_id.0,
                source_id,
                &SourceRevisionContent::observed(
                    AcquisitionId(format!("capture:{source_id}")),
                    files.iter().map(|(_, bytes)| bytes.len() as u64).sum(),
                    Some(digest.clone()),
                ),
                true,
            )
            .unwrap();
        let store = persistence_source::detail::open_store_for_write(&conn)
            .unwrap()
            .unwrap();
        persistence_source::fjall::replace_source_generation(
            &store,
            source_id,
            "source-facts",
            &digest,
        )
        .unwrap();
        drop(store);
        test_support::mark_source_projection_complete(&conn, source_id);
    }

    pub fn graph(&self) -> StructureGraph {
        self.try_graph().unwrap()
    }

    pub fn try_graph(
        &self,
    ) -> Result<StructureGraph, crate::structure_analysis::StructureAnalysisError> {
        crate::structure_facts::get_structure_fact_graph(
            &self.connection,
            self.root.path(),
            &self.case_id,
            &[],
        )
    }
}

fn file_entry(source: &DataSourceId, index: usize, path: &str, bytes: &[u8]) -> FileEntry {
    FileEntry {
        id: FileEntryId(format!("{}-file-{index}", source.0)),
        parent_id: None,
        data_source_id: source.clone(),
        path: path.into(),
        name: PathBuf::from(path)
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .into(),
        entry_type: EntryType::File,
        size: Some(bytes.len() as u64),
        ext: None,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: true,
        archive: false,
        unix_mode: Some(0o644),
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: Some(hex::encode(Sha256::digest(bytes))),
    }
}

pub fn host_files(serial: &str) -> Vec<(&str, &[u8])> {
    vec![
        (
            "sys/class/dmi/id/product_uuid",
            b"699ead5e-5739-4a12-b3a9-61ca15bbad1f",
        ),
        ("sys/class/dmi/id/board_serial", serial.as_bytes()),
        (
            "proc/sys/kernel/random/boot_id",
            b"a6304f81-dbab-4718-a0ce-90f379481eb3",
        ),
    ]
}

pub fn audit_event(node: &str, time: &str, version: &str) -> Value {
    json!({
        "apiVersion": "audit.k8s.io/v1", "kind": "Event",
        "auditID": format!("audit-{version}"), "stage": "ResponseComplete",
        "requestReceivedTimestamp": time, "stageTimestamp": time,
        "verb": "get", "responseStatus": {"code": 200},
        "objectRef": {"resource": "pods", "namespace": "app", "name": "worker", "uid": "pod-1"},
        "responseObject": {
            "apiVersion": "v1", "kind": "Pod",
            "metadata": {"uid": "pod-1", "name": "worker", "namespace": "app", "resourceVersion": version,
                         "ownerReferences": [{"uid":"replicaset-1","name":"worker","kind":"ReplicaSet"}]},
            "spec": {"nodeName": node}
        }
    })
}
