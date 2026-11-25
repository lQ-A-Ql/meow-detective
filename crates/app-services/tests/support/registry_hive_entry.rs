pub fn hive_entry(
    conn: &rusqlite::Connection,
    case_id: &str,
    source: &str,
    id: &str,
    path: &str,
    size: u64,
) {
    super::fixture::entry(conn, source, id, path, size);
    persistence_source::repositories::artifact_repo::ArtifactRepo::new(conn)
        .insert_batch(
            &[domain::Artifact {
                id: domain::ArtifactId(format!("hive-artifact:{id}")),
                family: "RegistryHive".into(),
                source_object_id: Some(domain::FileEntryId(id.to_string())),
                title: format!("Registry Hive: {path}"),
                summary: String::new(),
                extractor_id: Some("registry.hive.v1".into()),
                extractor_version: Some("2.1.0".into()),
                confidence: None,
                source_attribution: Some(path.into()),
                created_at: chrono::Utc::now(),
                attrs: std::collections::BTreeMap::from([
                    ("dataSourceId".into(), serde_json::json!(source)),
                    ("sourcePath".into(), serde_json::json!(path)),
                ]),
            }],
            case_id,
            source,
        )
        .unwrap();
}
