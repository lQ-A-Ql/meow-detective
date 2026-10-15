use super::{identity, load_source, DigestServiceError, DigestTarget};
use domain::CaseId;
use std::path::Path;
use transport::dto::{
    DigestAlgorithmDto, DigestResultDto, DigestResultsRequestDto, DigestScopeDto, DigestStatusDto,
    EvidenceDigestDto,
};
pub fn list(
    conn: &rusqlite::Connection,
    root: &Path,
    case: &CaseId,
    request: DigestResultsRequestDto,
) -> Result<Vec<DigestResultDto>, DigestServiceError> {
    load_source(conn, case, Some(&request.data_source_id))?;
    let records =
        persistence_sqlite::repositories::evidence_digest_repo::EvidenceDigestRepo::new(conn)
            .list(
                &case.0,
                &request.data_source_id,
                request.file_id.as_deref(),
                request.partition_index,
            )?;
    records
        .into_iter()
        .map(|record| {
            let target = DigestTarget {
                data_source_id: record.data_source_id.as_deref(),
                file_id: record.file_id.as_deref(),
                partition_index: record.partition_index,
            };
            let stale =
                identity::snapshot(conn, root, case, &target)?.revision != record.source_revision;
            let scope = match record.scope.as_str() {
                "ContainerFile" => DigestScopeDto::ContainerFile,
                "ContainerSet" => DigestScopeDto::ContainerSet,
                "LogicalDisk" => DigestScopeDto::LogicalDisk,
                "Partition" => DigestScopeDto::Partition,
                "File" => DigestScopeDto::File,
                "DerivedEvidence" => DigestScopeDto::DerivedEvidence,
                _ => return Err(DigestServiceError::InvalidInput),
            };
            let algorithm = match record.algorithm.as_str() {
                "md5" => DigestAlgorithmDto::Md5,
                "sha1" => DigestAlgorithmDto::Sha1,
                "sha256" => DigestAlgorithmDto::Sha256,
                "sm3" => DigestAlgorithmDto::Sm3,
                _ => return Err(DigestServiceError::InvalidInput),
            };
            Ok(DigestResultDto {
                digest: EvidenceDigestDto {
                    scope,
                    algorithm,
                    value: record.value,
                    byte_length: record.byte_length,
                    status: DigestStatusDto::Completed,
                },
                source_revision: record.source_revision,
                stale,
            })
        })
        .collect()
}
