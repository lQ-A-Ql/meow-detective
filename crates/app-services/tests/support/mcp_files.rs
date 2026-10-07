use std::{collections::HashMap, sync::Arc};

use app_services::{
    active_case::ActiveCase,
    bitlocker_runtime::BitLockerUnlockRegistry,
    case_service, file_service,
    mcp_host_service::{
        execute_file_tool, McpHostFileQueryContext, McpHostQueryContext, McpHostServiceError,
    },
    source_db::{self, GlobalFileId},
};
use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
use serde_json::Value;

pub struct Fixture {
    pub temporary: tempfile::TempDir,
    pub case: ActiveCase,
    pub registry: file_service::PreviewRuntimeRegistry,
    bitlocker: Arc<BitLockerUnlockRegistry>,
    ids: HashMap<String, String>,
    pub binary: Vec<u8>,
}

impl Fixture {
    pub fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let evidence = temporary.path().join("evidence");
        std::fs::create_dir_all(evidence.join("nested")).unwrap();
        std::fs::write(evidence.join("text.txt"), "证据内容\nHello MCP\n").unwrap();
        std::fs::write(evidence.join("empty.txt"), []).unwrap();
        std::fs::write(evidence.join("nested/detail.txt"), b"nested evidence").unwrap();
        let binary: Vec<_> = (0..130_001).map(|index| (index % 256) as u8).collect();
        std::fs::write(evidence.join("binary.bin"), &binary).unwrap();
        let case =
            case_service::create_case(&temporary.path().join("cases"), "MCP Files", None).unwrap();
        let source = DataSource {
            id: DataSourceId("mcp-files".into()),
            name: "evidence".into(),
            kind: DataSourceKind::LogicalDirectory,
            source_path: evidence,
            imported_at: chrono::Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(&source.id.0, Some("windows"), None);
        storage.import_state = "ready".into();
        DataSourceRepo::new(&case.connection().unwrap())
            .insert_with_storage(&case.meta.id, &source, &storage)
            .unwrap();
        let conn = source_db::open_source_db(&case.case_root, &source.id).unwrap();
        DataSourceRepo::new(&conn)
            .upsert_source_local_metadata(&case.meta.id, &source)
            .unwrap();
        let filesystem =
            evidence_core::LogicalFsReader::open(&source.source_path, "evidence").unwrap();
        file_service::enumerate_filesystem(&conn, &source.id, &filesystem).unwrap();
        let ids = FileRepo::new(&conn)
            .find_by_data_source(&source.id)
            .unwrap()
            .into_iter()
            .map(|entry| {
                (
                    entry.name,
                    GlobalFileId::new(source.id.clone(), entry.id).encode().0,
                )
            })
            .collect();
        Self {
            temporary,
            case,
            registry: Default::default(),
            bitlocker: Arc::default(),
            ids,
            binary,
        }
    }

    pub fn id(&self, name: &str) -> &str {
        &self.ids[name]
    }

    pub fn call(&self, name: &str, args: Value) -> Result<Value, McpHostServiceError> {
        let conn = self.case.connection().unwrap();
        let result = execute_file_tool(
            McpHostFileQueryContext {
                query: McpHostQueryContext {
                    connection: &conn,
                    case_root: &self.case.case_root,
                    case_meta: &self.case.meta,
                },
                preview_runtime: &self.registry,
                bitlocker_runtime: &self.bitlocker,
            },
            name,
            &args,
        );
        assert_eq!(
            self.registry.stats().unwrap().session_count,
            0,
            "Temporary handles must close after every call"
        );
        result
    }
}
