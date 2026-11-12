/// Minimal case bootstrap shared by unit tests whose legacy helper lived in the
/// application-service `case_service` before the evidence layer moved out.
pub(crate) fn create_case(root: &std::path::Path, name: &str) -> crate::active_case::ActiveCase {
    use chrono::Utc;
    use domain::{CaseId, CaseMeta};
    use persistence_sqlite::{open_or_create, repositories::case_repo::CaseRepo, runner};

    let case_root = root.join(name);
    for dir in ["evidence", "exports", "reports", "indexes", "cache", "logs"] {
        std::fs::create_dir_all(case_root.join(dir)).expect("create case directory");
    }
    let connection = open_or_create(&case_root.join("app.db")).expect("open case database");
    runner::run_all(&connection).expect("run case schema migrations");
    let now = Utc::now();
    let meta = CaseMeta {
        id: CaseId(uuid::Uuid::new_v4().to_string()),
        name: name.to_owned(),
        number: None,
        examiner: None,
        notes: None,
        created_at: now,
        updated_at: now,
    };
    CaseRepo::new(&connection)
        .create(&meta)
        .expect("create case record");
    crate::storage_manifest::initialize_case(&case_root, &meta.id.0)
        .expect("initialize case storage manifest");
    crate::active_case::ActiveCase::new(meta, case_root, connection)
}

#[macro_export]
macro_rules! native_source_test_support {
    () => {
        mod test_support {
            #![allow(dead_code)]

            use domain::FileEntry;
            use rusqlite::Connection;
            use std::ops::Deref;

            pub(crate) struct NativeSourceFixture {
                _root: tempfile::TempDir,
                pub(crate) connection: Connection,
                pub(crate) source_id: String,
                pub(crate) source_dir: std::path::PathBuf,
            }

            impl NativeSourceFixture {
                pub(crate) fn new(source_id: &str, case_id: &str) -> Self {
                    let root = tempfile::tempdir().expect("create native source root");
                    let case_root = root.path().join("case");
                    std::fs::create_dir_all(case_root.join("artifacts/objects"))
                        .expect("create case artifact storage");
                    let source_dir = case_root.join("sources").join(source_id);
                    std::fs::create_dir_all(&source_dir).expect("create native source directory");
                    let database_path = source_dir.join("source.db");
                    let connection = persistence_sqlite::open_or_create_source(&database_path)
                        .expect("open native source database");
                    persistence_source::detail::configure_source_identity(&connection, source_id)
                        .expect("configure native source identity");
                    persistence_source::detail::configure_source_fjall(&connection, &database_path)
                        .expect("configure native source details");
                    connection
                        .execute(
                            "INSERT INTO data_sources
                             (id, case_id, name, kind, source_path, imported_at)
                             VALUES (?1, ?2, ?1, 'directory', 'evidence',
                                     '2026-01-01T00:00:00Z')",
                            rusqlite::params![source_id, case_id],
                        )
                        .expect("register native source");
                    let fixture = Self {
                        _root: root,
                        connection,
                        source_id: source_id.to_owned(),
                        source_dir,
                    };
                    fixture.refresh_generation();
                    fixture
                }

                pub(crate) fn insert_files(&self, files: &[FileEntry]) {
                    persistence_source::repositories::file_repo::FileRepo::new(&self.connection)
                        .insert_batch(files)
                        .expect("insert native catalog records");
                    self.refresh_generation();
                }

                pub(crate) fn seal(&self) {
                    self.refresh_generation();
                }

                fn refresh_generation(&self) {
                    let store = persistence_source::detail::open_store_for_write(&self.connection)
                        .expect("open native source store")
                        .expect("configured native source store");
                    persistence_source::fjall::replace_source_generation(
                        &store,
                        &self.source_id,
                        "fixture",
                        &"a".repeat(64),
                    )
                    .expect("seal native source fixture");
                    drop(store);
                    mark_source_projection_complete(&self.connection, &self.source_id);
                }
            }

            impl Deref for NativeSourceFixture {
                type Target = Connection;

                fn deref(&self) -> &Self::Target {
                    &self.connection
                }
            }

            pub(crate) fn mark_source_projection_complete(
                connection: &Connection,
                source_id: &str,
            ) {
                let store = persistence_source::detail::open_store_for_write(connection)
                    .expect("open native source store")
                    .expect("configured native source store");
                let key = persistence_source::fjall::encode_key(
                    "projection",
                    &[persistence_source::fjall::KeyPart::Text(
                        source_id.to_owned(),
                    )],
                )
                .expect("encode projection key");
                let marker = persistence_source::fjall::VersionedEnvelope::new(
                    "source_projection_complete",
                    1,
                    format!("projection:{source_id}"),
                    source_id,
                    1,
                    source_id.as_bytes().to_vec(),
                );
                store
                    .write_envelope(
                        persistence_source::fjall::KeyspaceName::Meta,
                        key,
                        &marker,
                        persistence_source::fjall::PersistMode::SyncAll,
                    )
                    .expect("mark source projection complete");
            }
        }
    };
}
