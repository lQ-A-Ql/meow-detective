use app_services::{
    active_case::ActiveCase, bitlocker_runtime::BitLockerUnlockRegistry,
    file_service::PreviewRuntimeRegistry, registry_browser_service::RegistryBrowserContext,
};
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use std::{path::Path, sync::Arc};

pub struct BrowserCase {
    pub temp: tempfile::TempDir,
    pub active: ActiveCase,
    pub bitlocker: Arc<BitLockerUnlockRegistry>,
    pub preview: PreviewRuntimeRegistry,
}
impl BrowserCase {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let active = app_services::case_service::create_case(
            &temp.path().join("cases"),
            "registry-browser-test",
            None,
        )
        .unwrap();
        Self {
            temp,
            active,
            bitlocker: Arc::new(BitLockerUnlockRegistry::default()),
            preview: PreviewRuntimeRegistry::default(),
        }
    }
    pub fn context<'a>(&'a self, conn: &'a rusqlite::Connection) -> RegistryBrowserContext<'a> {
        RegistryBrowserContext {
            case_conn: conn,
            case_root: &self.active.case_root,
            case_id: &self.active.meta.id,
            bitlocker: &self.bitlocker,
            preview: &self.preview,
        }
    }
    pub fn source(
        &self,
        conn: &rusqlite::Connection,
        id: &str,
        path: &Path,
        kind: DataSourceKind,
    ) -> rusqlite::Connection {
        let source = DataSource {
            id: DataSourceId(id.into()),
            name: id.into(),
            source_path: path.to_path_buf(),
            kind,
            imported_at: chrono::Utc::now(),
            provenance: DataSourceProvenance::unknown(),
        };
        let mut storage = DataSourceStorage::source_db(id, Some("windows"), None);
        storage.import_state = "ready".into();
        DataSourceRepo::new(conn)
            .insert_with_storage(&self.active.meta.id, &source, &storage)
            .unwrap();
        let source_conn =
            app_services::source_db::open_source_db(&self.active.case_root, &source.id).unwrap();
        DataSourceRepo::new(&source_conn)
            .upsert_source_local_metadata(&CaseId(self.active.meta.id.0.clone()), &source)
            .unwrap();
        source_conn
    }
}
pub fn entry(conn: &rusqlite::Connection, source: &str, id: &str, path: &str, size: u64) {
    let name = path.rsplit('/').next().unwrap();
    conn.execute("INSERT INTO file_entries (id,data_source_id,path,name,entry_type,size,deleted,hidden,system,encrypted) VALUES (?1,?2,?3,?4,'file',?5,0,0,0,0)",rusqlite::params![id,source,path,name,size]).unwrap();
}
