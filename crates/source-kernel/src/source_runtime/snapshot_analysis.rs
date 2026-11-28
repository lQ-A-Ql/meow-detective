use std::{collections::BTreeSet, io, path::Path};

use domain::structure::{SnapshotAnalysisReceipt, SourceRevisionRef, StructureAttemptRef};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::{read_scope::SourceReadScope, SourceInvalidated};

pub struct SnapshotAnalysisSession {
    scope: SourceReadScope,
    target_id: String,
    input_capability_ids: BTreeSet<String>,
    generation_identity: String,
    attempt: StructureAttemptRef,
    verifier_id: String,
    policy_version: String,
}

impl SnapshotAnalysisSession {
    pub fn acquire(
        case_root: &Path,
        source: &SourceRevisionRef,
        target_id: String,
        input_capability_ids: BTreeSet<String>,
        attempt: StructureAttemptRef,
        verifier_id: String,
        policy_version: String,
    ) -> io::Result<Self> {
        for value in [
            target_id.as_str(),
            attempt.attempt_id.as_str(),
            attempt.owner.as_str(),
            verifier_id.as_str(),
            policy_version.as_str(),
        ] {
            if value.trim().is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "snapshot analysis identity is empty",
                ));
            }
        }
        if input_capability_ids.is_empty()
            || input_capability_ids.iter().any(|id| id.trim().is_empty())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "snapshot analysis input capability references are empty",
            ));
        }
        let scope = SourceReadScope::acquire(case_root, source)?;
        require_published_generation(case_root, source)?;
        let generation_identity = scope.generation_identity().to_owned();
        if generation_identity.trim().is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "snapshot analysis generation identity is empty",
            ));
        }
        scope.validate()?;
        Ok(Self {
            scope,
            target_id,
            input_capability_ids,
            generation_identity,
            attempt,
            verifier_id,
            policy_version,
        })
    }

    pub fn seal<T: Serialize>(self, output: &T) -> io::Result<SnapshotAnalysisProof> {
        self.scope.validate()?;
        let bytes = serde_json::to_vec(output).map_err(io::Error::other)?;
        let output_sha256 = hex::encode(Sha256::digest(&bytes));
        self.scope.validate()?;
        let receipt = SnapshotAnalysisReceipt::new(
            self.scope.source().clone(),
            self.scope.dependencies().to_vec(),
            self.target_id,
            self.input_capability_ids,
            self.generation_identity,
            self.scope.fingerprint().to_owned(),
            output_sha256,
            self.attempt,
            self.verifier_id,
            self.policy_version,
        )
        .map_err(io::Error::other)?;
        let proof = SnapshotAnalysisProof {
            receipt,
            scope: self.scope,
        };
        proof.validate()?;
        Ok(proof)
    }
}

pub struct SnapshotAnalysisProof {
    receipt: SnapshotAnalysisReceipt,
    scope: SourceReadScope,
}

impl std::fmt::Debug for SnapshotAnalysisProof {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SnapshotAnalysisProof")
            .field("receipt", &self.receipt)
            .finish_non_exhaustive()
    }
}

impl SnapshotAnalysisProof {
    pub fn receipt(&self) -> &SnapshotAnalysisReceipt {
        &self.receipt
    }

    pub fn validate(&self) -> io::Result<()> {
        self.scope.validate()?;
        self.receipt.validate().map_err(io::Error::other)?;
        if self.receipt.source_revision != *self.scope.source()
            || self.receipt.dependency_revisions.as_slice() != self.scope.dependencies()
            || self.receipt.generation_identity != self.scope.generation_identity()
            || self.receipt.scope_fingerprint != self.scope.fingerprint()
        {
            return Err(io::Error::other(SourceInvalidated));
        }
        self.scope.validate()
    }
}

fn require_published_generation(case_root: &Path, source: &SourceRevisionRef) -> io::Result<()> {
    let case = persistence_sqlite::open_existing_case_read_only(&case_root.join("app.db"))
        .map_err(io::Error::other)?;
    let source_id = domain::DataSourceId(source.source_id.clone());
    let storage = DataSourceRepo::new(&case)
        .find_storage(&source_id)
        .map_err(io::Error::other)?
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "snapshot analysis source is missing",
            )
        })?;
    if !matches!(storage.import_state.as_str(), "ready" | "ready_metadata") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "snapshot analysis requires a published source generation",
        ));
    }
    let control =
        crate::source_db::open_registered_source_db_read_only(&case, case_root, &source_id)
            .map_err(io::Error::other)?;
    let generation =
        persistence_source::detail::generation_name(&control).map_err(io::Error::other)?;
    if generation.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "snapshot analysis generation binding is empty",
        ));
    }
    Ok(())
}
