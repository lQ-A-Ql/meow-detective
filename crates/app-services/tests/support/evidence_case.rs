use domain::{DataSource, DataSourceId, DataSourceKind, DataSourceProvenance};
use persistence_sqlite::repositories::datasource_repo::{DataSourceRepo, DataSourceStorage};
use std::path::Path;
pub struct EvidenceCase {
    pub temp: tempfile::TempDir,
    pub active: app_services::active_case::ActiveCase,
}
impl EvidenceCase {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let active = app_services::case_service::create_case(
            &temp.path().join("cases"),
            "evidence-test",
            None,
        )
        .unwrap();
        Self { temp, active }
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
        let local =
            app_services::source_db::open_source_db(&self.active.case_root, &source.id).unwrap();
        DataSourceRepo::new(&local)
            .upsert_source_local_metadata(&self.active.meta.id, &source)
            .unwrap();
        local
    }
}
pub fn entry(conn: &rusqlite::Connection, source: &str, id: &str, path: &str, size: u64) {
    conn.execute("INSERT INTO file_entries(id,data_source_id,path,name,entry_type,size,deleted,hidden,system,encrypted) VALUES(?1,?2,?3,?4,'file',?5,0,0,0,0)",rusqlite::params![id,source,path,path.rsplit('/').next().unwrap(),size]).unwrap();
}
