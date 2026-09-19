use persistence_sqlite::repositories::ledger_repo::{
    LedgerBatch, LedgerBatchVerification, LedgerEntry, LedgerProof, LedgerVerification,
};
use transport::dto::{
    LedgerBatchDto, LedgerBatchVerificationDto, LedgerEntryDto, LedgerProofDto, LedgerProofStepDto,
    LedgerVerificationDto,
};

pub(super) fn entry_to_dto(entry: LedgerEntry) -> LedgerEntryDto {
    LedgerEntryDto {
        id: entry.id,
        scope_key: entry.scope_key,
        case_id: entry.case_id,
        audit_id: entry.audit_id,
        sequence: entry.sequence,
        actor_id: entry.actor_id,
        action: entry.action,
        resource_type: entry.resource_type,
        resource_id: entry.resource_id,
        details: entry.details,
        previous_hash: entry.previous_hash,
        entry_hash: entry.entry_hash,
        created_at: entry.created_at,
    }
}

pub(super) fn batch_to_dto(batch: LedgerBatch) -> LedgerBatchDto {
    LedgerBatchDto {
        id: batch.id,
        scope_key: batch.scope_key,
        case_id: batch.case_id,
        start_sequence: batch.start_sequence,
        end_sequence: batch.end_sequence,
        entry_count: batch.entry_count,
        merkle_root: batch.merkle_root,
        head_hash: batch.head_hash,
        created_at: batch.created_at,
    }
}

pub(super) fn verification_to_dto(
    verification: LedgerVerification,
    batches: LedgerBatchVerification,
) -> LedgerVerificationDto {
    LedgerVerificationDto {
        valid: verification.valid && batches.valid,
        entry_count: verification.entry_count,
        head_hash: verification.head_hash,
        first_error: verification.first_error.or(batches.first_error.clone()),
        batches: LedgerBatchVerificationDto {
            valid: batches.valid,
            batch_count: batches.batch_count,
            first_error: batches.first_error,
        },
    }
}

pub(super) fn proof_to_dto(proof: LedgerProof) -> LedgerProofDto {
    LedgerProofDto {
        merkle_root: proof.batch.merkle_root.clone(),
        batch: batch_to_dto(proof.batch),
        sequence: proof.sequence,
        entry_hash: proof.entry_hash,
        leaf_index: proof.proof.leaf_index,
        leaf_count: proof.proof.leaf_count,
        steps: proof
            .proof
            .steps
            .into_iter()
            .map(|step| LedgerProofStepDto {
                sibling_hash: step.sibling_hash,
                sibling_on_left: step.sibling_on_left,
            })
            .collect(),
    }
}
