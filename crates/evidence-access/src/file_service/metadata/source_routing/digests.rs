use crate::digest_service::{identity, DigestServiceError};
use crate::file_service::FileServiceError;
use domain::{CaseId, DataSourceId};
use persistence_sqlite::repositories::evidence_digest_repo::EvidenceDigestRepo;
use rusqlite::Connection;
use std::{collections::BTreeMap, path::Path};
use transport::dto::FileEntryRowDto;

pub(super) fn apply_file_sha256(
    case_conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
    source_id: &DataSourceId,
    rows: &mut [FileEntryRowDto],
) -> Result<(), FileServiceError> {
    for page in rows.chunks_mut(500) {
        let ids = page.iter().map(|row| row.id.as_str()).collect::<Vec<_>>();
        let values = EvidenceDigestRepo::new(case_conn)
            .file_sha256_values(&case_id.0, &source_id.0, &ids)?
            .into_iter()
            .map(|record| (record.file_id.clone(), record))
            .collect::<BTreeMap<_, _>>();
        let digest_ids = values.keys().map(String::as_str).collect::<Vec<_>>();
        let identities =
            identity::snapshot_files(case_conn, case_root, case_id, source_id, &digest_ids)
                .map_err(identity_error)?;
        for row in page {
            let Some(record) = values.get(&row.id) else {
                continue;
            };
            let current = identities
                .get(&row.id)
                .ok_or_else(|| FileServiceError::not_found("Digest target no longer exists"))?;
            if record.source_revision == current.revision {
                row.hash_sha256 = Some(record.value.clone());
            }
        }
    }
    Ok(())
}

fn identity_error(error: DigestServiceError) -> FileServiceError {
    match error {
        DigestServiceError::Database(error) => FileServiceError::Db(error),
        DigestServiceError::File(error) => error,
        DigestServiceError::NotFound => {
            FileServiceError::not_found("Digest target no longer exists")
        }
        DigestServiceError::InvalidInput => {
            FileServiceError::invalid_input("Invalid digest target")
        }
        DigestServiceError::EvidenceChanged => FileServiceError::integrity("Digest target changed"),
        DigestServiceError::Hash(crate::hash_service::EvidenceHashError::Io {
            kind,
            operation,
        }) => FileServiceError::Io(std::io::Error::new(kind, operation)),
        DigestServiceError::Hash(crate::hash_service::EvidenceHashError::EvidenceChanged) => {
            FileServiceError::integrity("Digest target changed")
        }
        DigestServiceError::Unsupported => {
            FileServiceError::Unsupported("Digest target is unsupported".into())
        }
        other => FileServiceError::other(other.to_string()),
    }
}
