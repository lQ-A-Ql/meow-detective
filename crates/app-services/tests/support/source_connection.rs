pub fn open(
    case_connection: &rusqlite::Connection,
    active: &app_services::active_case::ActiveCase,
    source_id: &domain::DataSourceId,
) -> rusqlite::Connection {
    let source =
        persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(case_connection)
            .find_by_case(&active.meta.id)
            .unwrap()
            .into_iter()
            .find(|source| source.id == *source_id)
            .unwrap();
    let connection = app_services::source_db::open_source_db(&active.case_root, source_id).unwrap();
    persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(&connection)
        .upsert_source_local_metadata(&active.meta.id, &source)
        .unwrap();
    connection
}
