use domain::structure::{AcquisitionId, SourceRevisionContent};
use persistence_sqlite::repositories::{
    case_repo::CaseRepo,
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    source_revision_repo::SourceRevisionRepo,
};

pub(crate) fn publish_source(
    source_connection: &rusqlite::Connection,
    logical_size: u64,
) -> rusqlite::Connection {
    let root = persistence_sqlite::connection::case_root_from_connection(source_connection)
        .unwrap()
        .unwrap();
    let source_id = domain::DataSourceId(
        persistence_source::detail::source_id(source_connection)
            .unwrap()
            .unwrap(),
    );
    let sources = DataSourceRepo::new(source_connection);
    let case_id = sources.case_id(&source_id).unwrap();
    let source = sources
        .find_by_case(&case_id)
        .unwrap()
        .into_iter()
        .find(|source| source.id == source_id)
        .unwrap();
    let connection = persistence_sqlite::open_or_create(&root.join("app.db")).unwrap();
    persistence_sqlite::runner::run_all(&connection).unwrap();
    CaseRepo::new(&connection)
        .create(&domain::CaseMeta {
            id: case_id.clone(),
            name: "Published source reads".into(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .unwrap();
    let mut storage = DataSourceStorage::source_db(&source_id.0, None, None);
    storage.import_state = "ready".into();
    DataSourceRepo::new(&connection)
        .insert_with_storage(&case_id, &source, &storage)
        .unwrap();
    let store = persistence_source::detail::require_source_store(source_connection).unwrap();
    let manifest =
        persistence_source::fjall::read_source_generation_manifest(&store, &source_id.0).unwrap();
    SourceRevisionRepo::new(&connection)
        .bind(
            &case_id.0,
            &source_id.0,
            &SourceRevisionContent::observed(
                AcquisitionId(source_id.0.clone()),
                logical_size,
                Some(format!("source-catalog:{}", manifest.immutable_data_digest)),
            ),
            true,
        )
        .unwrap();
    connection
}
