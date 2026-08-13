use super::*;
use crate::plugin_loader::PluginModuleMeta;

fn source_connection() -> Connection {
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    connection
}

fn plugin_meta(plugin_id: &str, families: &[&str]) -> PluginModuleMeta {
    PluginModuleMeta {
        plugin_id: plugin_id.to_string(),
        display_name: format!("Display {plugin_id}"),
        plugin_version: "1.0.0".to_string(),
        evidence_platform: "windows".to_string(),
        families: families.iter().map(|family| family.to_string()).collect(),
    }
}

fn insert_artifact(
    conn: &Connection,
    id: &str,
    plugin_id: &str,
    family: &str,
    title: &str,
    created_at: &str,
) {
    conn.execute(
        "INSERT INTO artifacts (
            id, case_id, data_source_id, artifact_type, source_object_id,
            extractor_id, extractor_version, confidence, source_attribution,
            title, summary, attrs, created_at
         ) VALUES (?1, 'case-1', 'source-1', ?2, 'file-1', ?3, '1.0.0', 0.9,
                   '[P0]/Evidence/FOO.MFX', ?4, 'summary text', '{\"k\":\"v\"}', ?5)",
        rusqlite::params![id, family, plugin_id, title, created_at],
    )
    .expect("insert plugin artifact");
}

#[test]
fn modules_group_counts_by_plugin_and_family() {
    let conn = source_connection();
    insert_artifact(
        &conn,
        "a1",
        "meow.a",
        "Prefetch",
        "t1",
        "2026-08-01T00:00:00Z",
    );
    insert_artifact(
        &conn,
        "a2",
        "meow.a",
        "Prefetch",
        "t2",
        "2026-08-01T00:01:00Z",
    );
    insert_artifact(
        &conn,
        "a3",
        "meow.a",
        "WeChat",
        "t3",
        "2026-08-01T00:02:00Z",
    );
    // Another plugin's artifacts must not leak into this module.
    insert_artifact(
        &conn,
        "b1",
        "meow.b",
        "Prefetch",
        "t4",
        "2026-08-01T00:03:00Z",
    );

    let metas = vec![
        plugin_meta("meow.a", &["Prefetch", "WeChat"]),
        plugin_meta("meow.b", &["Prefetch"]),
    ];
    let mut warnings = BTreeMap::new();
    warnings.insert("meow.a".to_string(), vec!["diag line".to_string()]);

    let modules = list_plugin_modules(&conn, &metas, &warnings).expect("list modules");
    assert_eq!(modules.len(), 2);
    let module_a = &modules[0];
    assert_eq!(module_a.plugin_id, "meow.a");
    assert_eq!(module_a.display_name, "Display meow.a");
    assert_eq!(module_a.plugin_version, "1.0.0");
    assert_eq!(module_a.evidence_platform, "windows");
    assert_eq!(module_a.total_count, 3);
    assert_eq!(module_a.warnings, vec!["diag line".to_string()]);
    assert_eq!(
        module_a
            .families
            .iter()
            .map(|family| (family.family.as_str(), family.count))
            .collect::<Vec<_>>(),
        vec![("Prefetch", 2), ("WeChat", 1)]
    );
    let module_b = &modules[1];
    assert_eq!(module_b.total_count, 1);
    assert!(module_b.warnings.is_empty());
}

#[test]
fn family_entries_paginate_with_family_total() {
    let conn = source_connection();
    for index in 0..5 {
        insert_artifact(
            &conn,
            &format!("a{index}"),
            "meow.a",
            "Prefetch",
            &format!("title-{index}"),
            &format!("2026-08-01T00:00:0{index}Z"),
        );
    }
    let meta = plugin_meta("meow.a", &["Prefetch"]);

    let first = get_plugin_family_entries(&conn, &meta, "Prefetch", 0, 2).expect("first page");
    assert_eq!(first.plugin_id, "meow.a");
    assert_eq!(first.family, "Prefetch");
    assert_eq!(first.total_count, 5);
    assert!(first.truncated);
    assert_eq!(first.entries.len(), 2);
    // Newest first; the entry mapping carries title/summary/confidence/attrs.
    assert_eq!(first.entries[0].artifact_id, "a4");
    assert_eq!(first.entries[0].title, "title-4");
    assert_eq!(first.entries[0].summary, "summary text");
    assert_eq!(first.entries[0].confidence, Some(0.9));
    assert_eq!(first.entries[0].file_id, "file-1");
    assert_eq!(first.entries[0].source_path, "[P0]/Evidence/FOO.MFX");
    assert_eq!(
        first.entries[0].attrs.get("k"),
        Some(&serde_json::Value::String("v".to_string()))
    );

    let second = get_plugin_family_entries(&conn, &meta, "Prefetch", 2, 2).expect("second page");
    assert_eq!(second.entries.len(), 2);
    assert!(second.truncated);
    assert_eq!(second.entries[0].artifact_id, "a2");

    let last = get_plugin_family_entries(&conn, &meta, "Prefetch", 4, 2).expect("last page");
    assert_eq!(last.entries.len(), 1);
    assert!(!last.truncated);
}

#[test]
fn undeclared_family_is_rejected() {
    let conn = source_connection();
    let meta = plugin_meta("meow.a", &["Prefetch"]);
    let result = get_plugin_family_entries(&conn, &meta, "WeChat", 0, 10);
    assert!(matches!(result, Err(AnalysisServiceError::InvalidInput(_))));
}

#[test]
fn empty_plugin_set_lists_no_modules() {
    let conn = source_connection();
    let modules = list_plugin_modules(&conn, &[], &BTreeMap::new()).expect("list modules");
    assert!(modules.is_empty());
}
