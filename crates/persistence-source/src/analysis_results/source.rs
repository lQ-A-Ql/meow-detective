use std::{
    fs, io,
    path::{Path, PathBuf},
};

use domain::{
    structure::{RevisionReadLease, SnapshotAnalysisReceipt, SourceRevisionRef},
    DataSourceId,
};
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, source_revision_repo::SourceRevisionRepo,
};
use rusqlite::Connection;

use super::{validate_receipt_id, AnalysisResultError, DIRECTORY_NAME};

pub(super) struct AnalysisSource {
    case: Connection,
    control: Connection,
    source: SourceRevisionRef,
    directory: PathBuf,
    dependencies: Vec<SourceRevisionRef>,
    leases: Vec<RevisionReadLease>,
}

impl AnalysisSource {
    pub fn acquire(
        case_root: &Path,
        source: &SourceRevisionRef,
    ) -> Result<Self, AnalysisResultError> {
        let case_path = crate::control::path_safety::safe_existing_case_path(
            case_root,
            &case_root.join("app.db"),
        )?;
        let case = persistence_sqlite::open_existing_case_read_only(&case_path)?;
        let source_id = DataSourceId(source.source_id.clone());
        require_case(&case, source)?;
        let dependencies = SourceRevisionRepo::new(&case).dependency_revisions(source)?;
        let leases = dependencies
            .iter()
            .flat_map(|reference| {
                [
                    RevisionReadLease::acquire(&reference.case_id, &reference.source_id),
                    RevisionReadLease::acquire_generation(&reference.case_id, &reference.source_id),
                ]
            })
            .collect();
        let database =
            crate::control::opening::registered_source_db_path(&case, case_root, &source_id)?;
        let database = crate::control::path_safety::safe_existing_case_path(case_root, &database)?;
        let directory = database
            .parent()
            .ok_or(AnalysisResultError::UnsafePath)?
            .to_path_buf();
        let control = crate::control::opening::open_registered_source_db_read_only(
            &case, case_root, &source_id,
        )?;
        let context = Self {
            case,
            control,
            source: source.clone(),
            directory,
            dependencies,
            leases,
        };
        context.validate()?;
        Ok(context)
    }

    pub fn revision(&self) -> &SourceRevisionRef {
        &self.source
    }

    pub fn require_receipt_inputs(
        &self,
        receipt: &SnapshotAnalysisReceipt,
    ) -> Result<(), AnalysisResultError> {
        self.validate()?;
        if receipt.source_revision != self.source
            || receipt.dependency_revisions != self.dependencies
        {
            return Err(AnalysisResultError::DependencyMismatch);
        }
        let generation = crate::control::bound_source_generation_root(
            &self.control,
            &DataSourceId(self.source.source_id.clone()),
        )?;
        if generation.canonicalize()?.parent() != Some(self.directory.as_path()) {
            return Err(AnalysisResultError::UnsafePath);
        }
        let runtime = crate::runtime::ReadOnlyStorageRuntime::open(&generation)?;
        let manifest = runtime
            .source()
            .generation_manifest(&self.source.source_id)?;
        let identity = serde_json::json!({
            "attempt": manifest.attempt_id,
            "immutable": manifest.immutable_data_digest,
            "projection": manifest.projection_batch_digest,
        })
        .to_string();
        if identity != receipt.generation_identity {
            return Err(AnalysisResultError::GenerationMismatch);
        }
        self.validate()?;
        Ok(())
    }

    pub fn result_directory(
        &self,
        receipt_id: &str,
        create: bool,
    ) -> Result<(PathBuf, bool), AnalysisResultError> {
        validate_receipt_id(receipt_id)?;
        self.validate()?;
        let parent = self.directory.join(DIRECTORY_NAME);
        if create {
            create_directory_if_absent(&parent)?;
        }
        require_local_directory(&parent)?;
        let path = parent.join(receipt_id);
        let created = create && create_directory_if_absent(&path)?;
        require_local_directory(&path)?;
        Ok((path, created))
    }

    fn validate(&self) -> Result<(), AnalysisResultError> {
        if self.leases.iter().any(|lease| !lease.is_current()) {
            return Err(AnalysisResultError::SourceChanged);
        }
        require_case(&self.case, &self.source)?;
        if crate::detail::source_id(&self.control)?.as_deref()
            != Some(self.source.source_id.as_str())
            || DataSourceRepo::new(&self.control)
                .case_id(&DataSourceId(self.source.source_id.clone()))?
                .0
                != self.source.case_id
        {
            return Err(AnalysisResultError::SourceOwnerMismatch);
        }
        if SourceRevisionRepo::new(&self.case).dependency_revisions(&self.source)?
            != self.dependencies
        {
            return Err(AnalysisResultError::DependencyMismatch);
        }
        Ok(())
    }
}

fn require_case(case: &Connection, source: &SourceRevisionRef) -> Result<(), AnalysisResultError> {
    if DataSourceRepo::new(case)
        .case_id(&DataSourceId(source.source_id.clone()))?
        .0
        != source.case_id
    {
        return Err(AnalysisResultError::SourceOwnerMismatch);
    }
    SourceRevisionRepo::new(case).require_current(source)?;
    Ok(())
}

fn create_directory_if_absent(path: &Path) -> Result<bool, AnalysisResultError> {
    match fs::create_dir(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn require_local_directory(path: &Path) -> Result<(), AnalysisResultError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        return Err(AnalysisResultError::UnsafePath);
    }
    let parent = path.parent().ok_or(AnalysisResultError::UnsafePath)?;
    let expected = parent
        .canonicalize()?
        .join(path.file_name().ok_or(AnalysisResultError::UnsafePath)?);
    if path.canonicalize()? != expected {
        return Err(AnalysisResultError::UnsafePath);
    }
    Ok(())
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}
