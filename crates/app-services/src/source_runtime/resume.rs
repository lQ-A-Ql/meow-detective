//! Bind disposable import work to the physical member metadata seen by its owner.
use evidence_core::EvidenceReaderFactory;
use persistence_sqlite::repositories::{file_repo::FileRepo, source_meta_repo::SourceMetaRepo};

const SOURCE_VIEW_KEY: &str = "import:physical-member-metadata:v1";

#[derive(Debug, thiserror::Error)]
pub(crate) enum ResumeIdentityError {
    #[error("Source members changed since import planning; re-import the evidence")]
    Changed,
    #[error("Existing import work has no verifiable source view; re-import the evidence")]
    Unbound,
    #[error("Live-device import work has no stable snapshot and cannot be resumed")]
    LiveDevice,
    #[error(transparent)]
    Db(#[from] persistence_sqlite::DbError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl transport::ServiceErrorCategory for ResumeIdentityError {
    fn category(&self) -> transport::ErrorCategory {
        match self {
            Self::Db(error) => transport::ServiceErrorCategory::category(error),
            Self::Io(_) => transport::ErrorCategory::Io,
            Self::LiveDevice => transport::ErrorCategory::Unsupported,
            Self::Changed | Self::Unbound => transport::ErrorCategory::Validation,
        }
    }
}

pub(crate) fn bind_source_view(
    conn: &rusqlite::Connection,
    source: &domain::DataSourceId,
    factory: &dyn EvidenceReaderFactory,
    case_root: &std::path::Path,
    initialize_view: bool,
) -> Result<(), ResumeIdentityError> {
    let repo = SourceMetaRepo::new(conn);
    let prior = repo.read(SOURCE_VIEW_KEY)?;
    // Existence matters even if an interrupted/corrupt manifest cannot deserialize.
    let has_work = crate::staging::staging_dir(case_root, &source.0).try_exists()?
        || FileRepo::new(conn).count_by_data_source(source)? > 1;
    match (factory.metadata_identity(), prior) {
        (Some(identity), Some(expected)) if identity == expected => Ok(()),
        (Some(_), Some(_)) => Err(ResumeIdentityError::Changed),
        (Some(_), None) if has_work || !initialize_view => Err(ResumeIdentityError::Unbound),
        (Some(identity), None) => Ok(repo.write(SOURCE_VIEW_KEY, identity)?),
        (None, _) if has_work || !initialize_view => Err(ResumeIdentityError::LiveDevice),
        (None, None) => Ok(()),
        (None, Some(_)) => Err(ResumeIdentityError::Changed),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/source_resume_identity.rs"]
mod tests;
