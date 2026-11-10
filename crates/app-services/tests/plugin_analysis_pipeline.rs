//! M2.5 analysis-chain integration: a real fixture plugin DLL flows through
//! candidate discovery, extraction, persistence and the generic summary DTOs
//! (module grouping + paged family entries).
#![cfg(windows)]

mod plugin_fixture_util;

use app_services::analysis_service::{
    discover_plugin_candidates, get_plugin_family_entries, list_plugin_modules,
};
use app_services::plugin_loader::load_plugins_from_dirs;
use artifacts_core::{ArtifactContext, ArtifactExtractor, VecSink};
use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, file_repo::FileRepo};
use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;

fn source_connection(root: &std::path::Path) -> rusqlite::Connection {
    let source_id = DataSourceId("source-1".into());
    let connection =
        app_services::source_db::open_source_db(root, &source_id).expect("open source database");
    connection
        .execute(
            "INSERT INTO data_sources
             (id, case_id, name, kind, source_path, imported_at)
             VALUES (
                 'source-1', 'case-1', 'Windows source', 'raw', 'test.e01',
                 '2026-08-01T00:00:00Z'
             )",
            [],
        )
        .expect("register source database owner");
    connection
}

#[test]
fn fixture_plugin_flows_from_discovery_to_summary() {
    let dir = plugin_fixture_util::stage_plugins(&["good"]);
    let plugins = load_plugins_from_dirs(&[dir.path().to_path_buf()]);
    assert_eq!(plugins.len(), 1);
    let plugin = &plugins[0];
    let plugin_refs: Vec<&dyn ArtifactExtractor> = plugins
        .iter()
        .map(|p| p as &dyn ArtifactExtractor)
        .collect();

    // 1. Candidate discovery: the declared "*.mfx" suffix pattern hits.
    let source_root = tempfile::tempdir().unwrap();
    let conn = source_connection(source_root.path());
    FileRepo::new(&conn)
        .insert_batch(&[FileEntry {
            id: FileEntryId("file-mfx".into()),
            parent_id: None,
            data_source_id: DataSourceId("source-1".into()),
            path: "[P0]/Evidence/FOO.MFX".into(),
            name: "FOO.MFX".into(),
            entry_type: EntryType::File,
            size: Some(64),
            ext: Some("mfx".into()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            read_only: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        }])
        .expect("insert plugin-matched file entry");
    let candidates = discover_plugin_candidates(&conn, &plugin_refs, &AtomicBool::new(false))
        .expect("discover plugin candidates");
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].category, "PluginArtifacts");

    // 2. Extraction through the registry-resolved plugin: provenance is
    //    host-enforced, extractor_id carries the bare plugin id.
    let mut sink = VecSink::new();
    let report = plugin
        .run(
            ArtifactContext {
                file_id: FileEntryId("file-mfx".to_string()),
                file_path: candidates[0].path.clone(),
                reader: Box::new(std::io::Cursor::new(vec![0u8; 16])),
            },
            &mut sink,
        )
        .expect("fixture extraction succeeds");
    assert_eq!(report.artifacts_found, 1);
    let artifact = &sink.artifacts[0];
    assert_eq!(artifact.extractor_id.as_deref(), Some("meow.fixture.good"));

    // 3. Persistence through the standard artifact pipeline.
    ArtifactRepo::new(&conn)
        .insert_batch(&sink.artifacts, "case-1", "source-1")
        .expect("persist plugin artifacts");

    // 4. Generic summary: one module, per-family counts, paged entries.
    let meta = plugin.module_meta();
    let modules = list_plugin_modules(&conn, std::slice::from_ref(&meta), &BTreeMap::new())
        .expect("list plugin modules");
    assert_eq!(modules.len(), 1);
    let module = &modules[0];
    assert_eq!(module.plugin_id, "meow.fixture.good");
    assert_eq!(module.display_name, "Fixture Good");
    assert_eq!(module.plugin_version, "0.1.0");
    assert_eq!(module.evidence_platform, "windows");
    assert_eq!(module.total_count, 1);
    assert_eq!(module.families.len(), 1);
    assert_eq!(module.families[0].family, "Fixture");
    assert_eq!(module.families[0].count, 1);

    let page = get_plugin_family_entries(&conn, &meta, "Fixture", 0, 10).expect("family entries");
    assert_eq!(page.total_count, 1);
    assert!(!page.truncated);
    assert_eq!(page.entries.len(), 1);
    let entry = &page.entries[0];
    assert_eq!(entry.title, "fixture artifact");
    assert_eq!(entry.file_id, "file-mfx");
    assert_eq!(entry.source_path, "[P0]/Evidence/FOO.MFX");
    assert_eq!(entry.confidence, Some(0.9));
    assert_eq!(
        entry.attrs.get("origin"),
        Some(&serde_json::Value::String("plugin".to_string()))
    );
}
