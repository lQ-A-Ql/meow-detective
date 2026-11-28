use domain::CaseId;
use persistence_sqlite::repositories::{
    environment_object_repo::{EnvironmentObjectRecord, EnvironmentObjectRepo},
    source_revision_repo::SourceRevisionRepo,
};

pub(crate) fn index_source_scopes(
    conn: &rusqlite::Connection,
    case_id: &CaseId,
    sources: &[(u32, String, String)],
) -> persistence_sqlite::DbResult<()> {
    let repo = EnvironmentObjectRepo::new(conn);
    for (_, source_id, name) in sources {
        let reference = SourceRevisionRepo::new(conn)
            .current(&case_id.0, source_id)?
            .0;
        repo.upsert(&EnvironmentObjectRecord {
            id: format!("source:{source_id}@{}", reference.revision),
            case_id: case_id.0.clone(),
            object_kind: "os_instance".into(),
            name: name.clone(),
            identity_state: "candidate".into(),
            status: "ready".into(),
            provenance_json: serde_json::json!({
                "dataSourceId": source_id,
                "sourceRevision": reference,
                "projection": "source_revision_index",
            })
            .to_string(),
        })?;
    }
    Ok(())
}
