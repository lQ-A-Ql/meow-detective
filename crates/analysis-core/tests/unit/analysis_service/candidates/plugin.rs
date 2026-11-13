use super::*;
use artifacts_core::{ArtifactContext, ArtifactSink, ExtractorReport};
use domain::ArtifactFamily;

/// Minimal plugin-shaped extractor stub: id + suffix path matching, no DLL.
struct StubPlugin {
    id: &'static str,
    suffix: &'static str,
}

impl ArtifactExtractor for StubPlugin {
    fn id(&self) -> &'static str {
        self.id
    }

    fn display_name(&self) -> &'static str {
        self.id
    }

    fn family(&self) -> ArtifactFamily {
        ArtifactFamily {
            name: "Stub".to_string(),
            description: None,
        }
    }

    fn supports_path(&self, file_path: &str) -> bool {
        file_path.to_lowercase().ends_with(self.suffix)
    }

    fn run(
        &self,
        _ctx: ArtifactContext,
        _sink: &mut dyn ArtifactSink,
    ) -> Result<ExtractorReport, String> {
        Ok(ExtractorReport {
            artifacts_found: 0,
            timeline_events: 0,
            errors: Vec::new(),
        })
    }
}

fn source_connection() -> Connection {
    let root = tempfile::tempdir().expect("create source root").keep();
    source_kernel::source_db::open_source_db(&root, &domain::DataSourceId("source-1".into()))
        .expect("open source database")
}

fn insert_file(conn: &Connection, id: &str, path: &str, name: &str) {
    persistence_source::repositories::file_repo::FileRepo::new(conn)
        .insert_batch(&[domain::FileEntry {
            id: domain::FileEntryId(id.into()),
            parent_id: None,
            data_source_id: domain::DataSourceId("source-1".into()),
            path: path.into(),
            name: name.into(),
            entry_type: domain::EntryType::File,
            size: Some(64),
            ext: None,
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
        .expect("insert file entry");
}

#[test]
fn plugin_patterns_discover_candidates_with_plugin_category() {
    let conn = source_connection();
    insert_file(&conn, "f1", "[P0]/Evidence/FOO.MFX", "FOO.MFX");
    insert_file(&conn, "f2", "[P0]/Evidence/notes.txt", "notes.txt");
    let stub = StubPlugin {
        id: "meow.stub",
        suffix: ".mfx",
    };
    let plugins: Vec<&dyn ArtifactExtractor> = vec![&stub as &dyn ArtifactExtractor];

    let candidates = discover_plugin_candidates(&conn, &plugins, &AtomicBool::new(false))
        .expect("discover plugin candidates");

    assert_eq!(candidates.len(), 1);
    let candidate = &candidates[0];
    assert_eq!(candidate.file_id.0, "f1");
    assert_eq!(candidate.category, PLUGIN_CAPABILITY_KEY);
    assert_eq!(candidate.parser, "meow.stub");
    assert_eq!(candidate.evidence_kind, "plugin");
    assert!(!candidate.content_identity.is_empty());
}

#[test]
fn empty_plugin_set_discovers_nothing() {
    let conn = source_connection();
    insert_file(&conn, "f1", "[P0]/Evidence/FOO.MFX", "FOO.MFX");
    let candidates = discover_plugin_candidates(&conn, &[], &AtomicBool::new(false))
        .expect("discover with no plugins");
    assert!(candidates.is_empty());
}

#[test]
fn database_candidate_carries_wal_companion_and_identity() {
    let conn = source_connection();
    insert_file(
        &conn,
        "db",
        "[P0]/Users/a/xwechat_files/wxid/db_storage/message/message_0.db",
        "message_0.db",
    );
    insert_file(
        &conn,
        "wal",
        "[P0]/Users/a/xwechat_files/wxid/db_storage/message/message_0.db-wal",
        "message_0.db-wal",
    );
    let stub = StubPlugin {
        id: "meow.stub",
        suffix: ".db",
    };
    let plugins: Vec<&dyn ArtifactExtractor> = vec![&stub];

    let candidates = discover_plugin_candidates(&conn, &plugins, &AtomicBool::new(false))
        .expect("discover database candidate");

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].companions.len(), 1);
    assert_eq!(candidates[0].companions[0].file_id.0, "wal");
    assert!(candidates[0]
        .content_identity
        .starts_with("plugin-candidate-v2:"));
}

fn partitioned_record(id: &str, partition: usize, path: &str) -> PluginFileRecord {
    PluginFileRecord {
        file_id: id.into(),
        data_source_id: "source".into(),
        partition_index: Some(partition),
        path: path.into(),
        size: 64,
        encrypted: false,
        content_identity: id.into(),
        modified_at: None,
    }
}

#[test]
fn wal_selection_requires_the_primary_partition() {
    let records = vec![
        partitioned_record("primary", 1, "[P1]/data/app.db"),
        partitioned_record("wal-one", 1, "[P1]/data/app.db-wal"),
        partitioned_record("wal-two", 2, "[P2]/data/app.db-wal"),
    ];
    let selected = companion_records(&records[0], &records, &companion_index(&records)).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].file_id.0, "wal-one");
}

#[test]
fn duplicate_wal_in_one_partition_is_rejected() {
    let records = vec![
        partitioned_record("primary", 1, "[P1]/data/app.db"),
        partitioned_record("wal-one", 1, "[P1]/data/app.db-wal"),
        partitioned_record("wal-two", 1, "[P1]/data/app.db-wal"),
    ];
    assert!(matches!(
        companion_records(&records[0], &records, &companion_index(&records)),
        Err(AnalysisServiceError::InvalidInput(_))
    ));
}
