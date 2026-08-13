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
    let connection = persistence_sqlite::open_in_memory().expect("open source database");
    persistence_sqlite::runner::run_source_all(&connection).expect("run source migrations");
    connection
}

fn insert_file(conn: &Connection, id: &str, path: &str, name: &str) {
    conn.execute(
        "INSERT INTO file_entries (
            id, parent_id, data_source_id, path, name, entry_type, size,
            deleted, hidden, system, encrypted
         ) VALUES (?1, NULL, 'source-1', ?2, ?3, 'file', 64, 0, 0, 0, 0)",
        rusqlite::params![id, path, name],
    )
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
