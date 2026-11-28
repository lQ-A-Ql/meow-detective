mod physical;
mod reader;
mod snapshot;

use std::{
    io,
    path::Path,
    sync::{Arc, Mutex},
};

use domain::structure::SourceRevisionRef;
use persistence_source::runtime::ReadOnlySourceStorageHandle;
use persistence_sqlite::repositories::source_revision_repo::SourceRevisionRepo;

use super::lease::SourceReadLease;

pub use reader::ScopedEvidenceReader;

#[derive(Clone)]
pub struct SourceReadScope(Arc<Scope>);

struct Scope {
    database: Mutex<Database>,
    source: SourceRevisionRef,
    fingerprint: String,
    dependencies: Vec<SourceRevisionRef>,
    leases: Vec<SourceReadLease>,
    generation: Option<Generation>,
    manifest_identity: String,
    physical: Option<physical::PhysicalScope>,
    _reservation: crate::resource_budget::ResourcePermit,
}

struct Generation {
    native: ReadOnlySourceStorageHandle,
    binding: String,
    control: Mutex<rusqlite::Connection>,
}

struct Database {
    connection: rusqlite::Connection,
    snapshot: snapshot::ScopeSnapshot,
    data_version: Option<u64>,
}

impl SourceReadScope {
    pub fn acquire(case_root: &Path, source: &SourceRevisionRef) -> io::Result<Self> {
        let connection =
            persistence_sqlite::open_existing_case_read_only(&case_root.join("app.db"))
                .map_err(io::Error::other)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(io::Error::other)?;
        let dependencies = SourceRevisionRepo::new(&transaction)
            .dependency_revisions(source)
            .map_err(io::Error::other)?;
        let reservation = crate::resource_budget::global_pool()
            .acquire(
                crate::resource_budget::Resources {
                    bytes: 64 * 1024 + dependencies.len() as u64 * 4096,
                    ..Default::default()
                },
                None,
            )
            .map_err(crate::resource_budget::ResourceError::into_io)?;
        let leases = dependencies
            .iter()
            .map(|reference| SourceReadLease::acquire(&reference.case_id, &reference.source_id))
            .collect::<Vec<_>>();
        let snapshot = snapshot::capture(&transaction, source, &dependencies)?;
        let fingerprint = snapshot::fingerprint(&snapshot)?;
        transaction.commit().map_err(io::Error::other)?;
        let (generation, manifest_identity) = open_generation(&connection, case_root, source)?;
        let scope = Self(Arc::new(Scope {
            database: Mutex::new(Database {
                connection,
                snapshot,
                data_version: None,
            }),
            source: source.clone(),
            fingerprint,
            dependencies,
            leases,
            generation,
            manifest_identity,
            physical: None,
            _reservation: reservation,
        }));
        scope.validate()?;
        Ok(scope)
    }

    pub fn with_backing_files(mut self, paths: &[std::path::PathBuf]) -> io::Result<Self> {
        let physical = physical::PhysicalScope::acquire(paths)?;
        let scope = Arc::get_mut(&mut self.0)
            .ok_or_else(|| io::Error::other("physical scope must be bound before sharing"))?;
        scope.fingerprint = format!("{}:{}", scope.fingerprint, physical.fingerprint());
        scope.physical = Some(physical);
        self.validate()?;
        Ok(self)
    }

    pub fn source(&self) -> &SourceRevisionRef {
        &self.0.source
    }
    pub fn dependencies(&self) -> &[SourceRevisionRef] {
        &self.0.dependencies
    }
    pub fn fingerprint(&self) -> &str {
        &self.0.fingerprint
    }
    pub fn generation_identity(&self) -> &str {
        &self.0.manifest_identity
    }

    pub fn validate(&self) -> io::Result<()> {
        self.validate_epoch()?;
        let mut database = self
            .0
            .database
            .lock()
            .map_err(|_| io::Error::other("source scope lock poisoned"))?;
        let data_version = snapshot::data_version(&database.connection)?;
        if Some(data_version) != database.data_version {
            let transaction = database
                .connection
                .unchecked_transaction()
                .map_err(io::Error::other)?;
            let current = snapshot::capture(&transaction, &self.0.source, &self.0.dependencies)?;
            if current != database.snapshot {
                return Err(io::Error::other(super::lease::SourceInvalidated));
            }
            transaction.commit().map_err(io::Error::other)?;
            database.data_version = Some(data_version);
        }
        drop(database);
        if let Some(generation) = &self.0.generation {
            let control = generation
                .control
                .lock()
                .map_err(|_| io::Error::other("source control lock poisoned"))?;
            if persistence_source::detail::generation_name(&control).map_err(io::Error::other)?
                != generation.binding
                || snapshot::manifest_identity(&generation.native, &self.0.source.source_id)?
                    != self.0.manifest_identity
            {
                return Err(io::Error::other(super::lease::SourceInvalidated));
            }
        }
        self.validate_epoch()
    }

    pub fn validate_epoch(&self) -> io::Result<()> {
        if let Some(physical) = &self.0.physical {
            physical.validate()?;
        }
        for lease in &self.0.leases {
            lease.validate()?;
        }
        Ok(())
    }
    pub fn wrap(
        &self,
        reader: Box<dyn evidence_core::EvidenceReader>,
    ) -> io::Result<Box<dyn evidence_core::EvidenceReader>> {
        self.validate()?;
        Ok(Box::new(ScopedEvidenceReader::new(reader, self.clone())))
    }
}

fn open_generation(
    conn: &rusqlite::Connection,
    root: &Path,
    source: &SourceRevisionRef,
) -> io::Result<(Option<Generation>, String)> {
    let source_id = domain::DataSourceId(source.source_id.clone());
    let storage = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn)
        .find_storage(&source_id)
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::other("source registration is missing"))?;
    if !matches!(storage.import_state.as_str(), "ready" | "ready_metadata") {
        return Ok((
            None,
            format!("verified-physical-revision:{}", source.revision),
        ));
    }
    persistence_source::publication::catalog_publication::verify_registered_catalog(
        conn,
        root,
        &domain::CaseId(source.case_id.clone()),
        &source_id,
    )
    .map_err(io::Error::other)?;
    let control = crate::source_db::open_registered_source_db_read_only(conn, root, &source_id)
        .map_err(io::Error::other)?;
    let binding =
        persistence_source::detail::generation_name(&control).map_err(io::Error::other)?;
    let native = persistence_source::runtime::open_source_read_only(root, &source.source_id)
        .map_err(io::Error::other)?;
    let identity = snapshot::manifest_identity(&native, &source.source_id)?;
    Ok((
        Some(Generation {
            native,
            binding,
            control: Mutex::new(control),
        }),
        identity,
    ))
}
