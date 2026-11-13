use app_services::{
    active_case::ActiveCase,
    case_service,
    import_pipeline::{execute_import_job, ImportJobOptions},
};
use domain::{DataSourceId, FileEntry};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, job_repo::JobRepo};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{atomic::AtomicBool, Arc},
};

pub(super) struct NativeFixture {
    pub active: ActiveCase,
    pub sources: Vec<DataSourceId>,
    _root: tempfile::TempDir,
}

impl NativeFixture {
    pub fn new(source_count: usize, files_per_source: usize) -> Self {
        assert!(source_count > 0 && files_per_source > 0);
        let root = tempfile::tempdir().unwrap();
        let active =
            case_service::create_case(&root.path().join("cases"), "performance", None).unwrap();
        let mut sources = Vec::with_capacity(source_count);
        for index in 0..source_count {
            let evidence = root.path().join(format!("evidence-{index:03}"));
            std::fs::create_dir(&evidence).unwrap();
            for file in 0..files_per_source {
                let path = evidence.join(format!("item-{file:08}.txt"));
                std::fs::write(path, format!("benchmark content {index} {file}\n")).unwrap();
            }
            let source = import_source(&active, &evidence);
            let connection =
                app_services::source_db::open_source_db(&active.case_root, &source).unwrap();
            assert_eq!(
                FileRepo::new(&connection).count_all().unwrap(),
                files_per_source as u64 + 1,
                "imported fixture must contain every file and its directory root"
            );
            sources.push(source);
        }
        Self {
            active,
            sources,
            _root: root,
        }
    }

    pub fn source_connection(&self, index: usize) -> rusqlite::Connection {
        app_services::source_db::open_source_db(&self.active.case_root, &self.sources[index])
            .unwrap()
    }

    pub fn files(&self, index: usize) -> Vec<FileEntry> {
        let connection = self.source_connection(index);
        let mut files = Vec::new();
        FileRepo::new(&connection)
            .visit_all(None, |entry| {
                if entry.is_file() {
                    files.push(entry);
                }
                Ok(())
            })
            .unwrap();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        files
    }
}

fn import_source(active: &ActiveCase, path: &Path) -> DataSourceId {
    active
        .with_conn(|connection| {
            let before = DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .into_iter()
                .map(|source| source.id.0)
                .collect::<BTreeSet<_>>();
            let job = JobRepo::new(connection).create(&active.meta.id.0, "performance fixture")?;
            let config = app_services::import_precheck::prepare_import_source_config_from_path(
                &path.to_string_lossy(),
                domain::DataSourcePlatform::Windows,
            )
            .unwrap();
            execute_import_job(
                connection,
                &active.meta.id,
                &active.case_root,
                config,
                &job,
                ImportJobOptions {
                    projection_policy: Default::default(),
                    event_sink: None,
                    cancel_token: &Arc::new(AtomicBool::new(false)),
                    max_import_workers: Some(1),
                    max_analysis_workers: Some(1),
                    analysis_mode: app_services::import_analysis::ImportAnalysisMode::MetadataOnly,
                    defer_linux_artifacts: false,
                },
            )
            .unwrap();
            let created = DataSourceRepo::new(connection)
                .find_by_case(&active.meta.id)?
                .into_iter()
                .filter(|source| !before.contains(&source.id.0))
                .collect::<Vec<_>>();
            assert_eq!(created.len(), 1);
            Ok(created.into_iter().next().unwrap().id)
        })
        .unwrap()
}
