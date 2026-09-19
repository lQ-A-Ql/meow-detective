use super::dto_conversion::{batch_to_dto, entry_to_dto, proof_to_dto, verification_to_dto};
use super::error::LedgerServiceError;
use persistence_sqlite::repositories::ledger_repo::LedgerRepo;
use rusqlite::Connection;
use transport::dto::{
    GetLedgerProofRequest, GetLedgerRequest, LedgerBatchDto, LedgerProofDto, LedgerSnapshotDto,
};

const DEFAULT_LEDGER_PAGE_SIZE: u32 = 100;
const MAX_LEDGER_PAGE_SIZE: u32 = 500;

pub fn get_snapshot(
    conn: &Connection,
    case_id: &str,
    request: GetLedgerRequest,
) -> Result<LedgerSnapshotDto, LedgerServiceError> {
    let limit = request.limit.unwrap_or(DEFAULT_LEDGER_PAGE_SIZE);
    if limit == 0 || limit > MAX_LEDGER_PAGE_SIZE {
        return Err(LedgerServiceError::Invalid(format!(
            "ledger page size must be between 1 and {MAX_LEDGER_PAGE_SIZE}"
        )));
    }
    let repo = LedgerRepo::new(conn);
    let verification = repo.verify(Some(case_id))?;
    let batches = repo.verify_batches(Some(case_id))?;
    Ok(LedgerSnapshotDto {
        entries: repo
            .list_entries(Some(case_id), limit, request.offset.unwrap_or(0))?
            .into_iter()
            .map(entry_to_dto)
            .collect(),
        batches: repo
            .list_batches(Some(case_id))?
            .into_iter()
            .map(batch_to_dto)
            .collect(),
        verification: verification_to_dto(verification, batches),
    })
}

pub fn seal_next_batch(
    conn: &Connection,
    case_id: &str,
) -> Result<Option<LedgerBatchDto>, LedgerServiceError> {
    Ok(LedgerRepo::new(conn)
        .seal_next_batch(Some(case_id))?
        .map(batch_to_dto))
}

pub fn get_proof(
    conn: &Connection,
    case_id: &str,
    request: GetLedgerProofRequest,
) -> Result<LedgerProofDto, LedgerServiceError> {
    LedgerRepo::new(conn)
        .proof(Some(case_id), &request.batch_id, request.sequence)?
        .map(proof_to_dto)
        .ok_or(LedgerServiceError::NotFound)
}
