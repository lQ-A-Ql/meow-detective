use std::{
    collections::BTreeMap,
    io::{self, Read},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use domain::structure::{ReadDemand, StructureAttemptRef, VerificationReceipt};
use evidence_core::{reader::ReaderResourceHold, EvidenceReader};
use sha2::{Digest, Sha256};

use super::{
    range_view::{check_active, VerifiedView},
    read_scope::SourceReadScope,
};

const MAX_RECEIPTS: usize = 4096;
type Entry = Arc<Mutex<ReceiptState>>;

enum ReceiptState {
    Pending,
    Ready(Arc<ValidatedReceipt>),
    Failed,
}

pub struct ValidatedReceipt {
    receipt: VerificationReceipt,
    scope: SourceReadScope,
    _reservation: Option<Arc<dyn ReaderResourceHold>>,
    failed: Arc<AtomicBool>,
}

impl ValidatedReceipt {
    pub fn receipt(&self) -> &VerificationReceipt {
        &self.receipt
    }
    pub fn validate(&self) -> io::Result<()> {
        self.validate_epoch()?;
        self.scope.validate()
    }

    pub fn validate_epoch(&self) -> io::Result<()> {
        if self.failed.load(Ordering::Acquire) {
            return Err(io::Error::other("verification window failed"));
        }
        self.scope.validate_epoch()
    }
}

pub struct VerificationBatch {
    attempt: StructureAttemptRef,
    entries: Mutex<BTreeMap<String, Entry>>,
    failures: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
    verified_bytes: AtomicU64,
    reused_receipts: AtomicU64,
    deep: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationBatchStats {
    pub verified_bytes: u64,
    pub reused_receipts: u64,
    pub source_scopes: usize,
}

pub struct VerificationRequest<'request> {
    pub demand: &'request ReadDemand,
    pub adapter_version: &'request str,
    pub policy_version: &'request str,
    pub expected_sha256: Option<&'request str>,
    pub fresh: bool,
}

impl VerificationBatch {
    pub fn new(attempt: Option<StructureAttemptRef>) -> io::Result<Self> {
        let attempt = attempt.unwrap_or_else(|| StructureAttemptRef {
            attempt_id: uuid::Uuid::new_v4().to_string(),
            owner: "read-verification".into(),
        });
        if attempt.attempt_id.is_empty() || attempt.owner.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "verification owner is missing",
            ));
        }
        Ok(Self {
            attempt,
            entries: Mutex::new(BTreeMap::new()),
            failures: Mutex::new(BTreeMap::new()),
            verified_bytes: AtomicU64::new(0),
            reused_receipts: AtomicU64::new(0),
            deep: false,
        })
    }

    pub fn new_deep(attempt: Option<StructureAttemptRef>) -> io::Result<Self> {
        let mut batch = Self::new(attempt)?;
        batch.deep = true;
        Ok(batch)
    }

    pub fn requires_input_verification(&self) -> bool {
        self.deep
    }

    pub fn stats(&self) -> io::Result<VerificationBatchStats> {
        Ok(VerificationBatchStats {
            verified_bytes: self.verified_bytes.load(Ordering::Acquire),
            reused_receipts: self.reused_receipts.load(Ordering::Acquire),
            source_scopes: self.failures.lock().map_err(lock_error)?.len(),
        })
    }

    pub fn verify(
        &self,
        scope: SourceReadScope,
        request: VerificationRequest<'_>,
        cancel: &AtomicBool,
        open: impl FnOnce() -> io::Result<Box<dyn EvidenceReader>>,
    ) -> io::Result<Arc<ValidatedReceipt>> {
        check_active(cancel, request.demand.deadline_unix_ms)?;
        if request.demand.verification == domain::structure::ReadVerification::NativeChecksum {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "native-checksum verification requires an admitted native verifier",
            ));
        }
        scope.validate()?;
        let failed = {
            let mut failures = self.failures.lock().map_err(lock_error)?;
            if failures.len() >= MAX_RECEIPTS && !failures.contains_key(scope.fingerprint()) {
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "verification scope bound exceeded",
                ));
            }
            failures
                .entry(scope.fingerprint().into())
                .or_default()
                .clone()
        };
        if failed.load(Ordering::Acquire) {
            return Err(io::Error::other("verification window failed"));
        }
        if request.demand.source_revision != *scope.source()
            || request
                .demand
                .attempt
                .as_ref()
                .is_some_and(|attempt| *attempt != self.attempt)
            || request.adapter_version.is_empty()
            || request.policy_version.is_empty()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "verification scope or owner differs",
            ));
        }
        let key = key(&scope, &request, &self.attempt)?;
        let entry = {
            let mut entries = self.entries.lock().map_err(lock_error)?;
            if entries.len() >= MAX_RECEIPTS && !entries.contains_key(&key) {
                return Err(io::Error::new(
                    io::ErrorKind::OutOfMemory,
                    "verification receipt bound exceeded",
                ));
            }
            entries
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(ReceiptState::Pending)))
                .clone()
        };
        let mut state = entry.lock().map_err(lock_error)?;
        match &*state {
            ReceiptState::Failed => {
                return Err(io::Error::other("verification failed in this owner window"))
            }
            ReceiptState::Ready(receipt) if !request.fresh => {
                receipt.validate()?;
                check_active(cancel, request.demand.deadline_unix_ms)?;
                self.reused_receipts.fetch_add(1, Ordering::AcqRel);
                return Ok(receipt.clone());
            }
            _ => {}
        }
        let reference = scope.source().clone();
        let result = self.build_receipt(scope, &request, &key, failed.clone(), cancel, open);
        if let Err(error) = &result {
            failed.store(true, Ordering::Release);
            if matches!(
                error.kind(),
                io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof
            ) {
                super::invalidate_source(&reference.case_id, &reference.source_id);
            }
        }
        *state = match &result {
            Ok(receipt) => ReceiptState::Ready(receipt.clone()),
            Err(_) => ReceiptState::Failed,
        };
        result
    }

    fn build_receipt(
        &self,
        scope: SourceReadScope,
        request: &VerificationRequest<'_>,
        key: &str,
        failed: Arc<AtomicBool>,
        cancel: &AtomicBool,
        open: impl FnOnce() -> io::Result<Box<dyn EvidenceReader>>,
    ) -> io::Result<Arc<ValidatedReceipt>> {
        let reader = scope.wrap(open()?)?;
        let reservation = match reader.reserve_work(1024 * 1024)? {
            Some(reservation) => reservation,
            None => Arc::new(
                crate::resource_budget::global_pool()
                    .acquire(
                        crate::resource_budget::Resources {
                            bytes: 1024 * 1024,
                            ..Default::default()
                        },
                        None,
                    )
                    .map_err(crate::resource_budget::ResourceError::into_io)?,
            ) as Arc<dyn ReaderResourceHold>,
        };
        let mut view = VerifiedView::new(
            reader,
            request.demand.ranges.clone(),
            cancel,
            request.demand.deadline_unix_ms,
        )?;
        let (actual, bytes_verified) =
            read_digest(&mut view, request.demand, cancel, &self.verified_bytes)?;
        drop(view);
        drop(reservation);
        if request
            .expected_sha256
            .is_some_and(|expected| actual != expected)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "verification output digest mismatch",
            ));
        }
        if bytes_verified != request.demand.ranges.covered_bytes() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "verification coverage is incomplete",
            ));
        }
        scope.validate()?;
        check_active(cancel, request.demand.deadline_unix_ms)?;
        let receipt = VerificationReceipt {
            id: key.into(),
            source_revision: scope.source().clone(),
            dependency_revisions: scope.dependencies().to_vec(),
            scope_fingerprint: scope.fingerprint().into(),
            generation_identity: scope.generation_identity().into(),
            target_fingerprint: request.demand.target_fingerprint.clone(),
            adapter_version: request.adapter_version.into(),
            policy_version: request.policy_version.into(),
            ranges: request.demand.ranges.clone(),
            bytes_verified,
            output_sha256: actual,
            expected_sha256: request.expected_sha256.map(str::to_owned),
            verification: request.demand.verification,
            attempt: self.attempt.clone(),
        };
        let reservation = crate::resource_budget::global_pool()
            .acquire(
                crate::resource_budget::Resources {
                    bytes: 4096 + scope.dependencies().len() as u64 * 256,
                    ..Default::default()
                },
                None,
            )
            .map_err(crate::resource_budget::ResourceError::into_io)?;
        Ok(Arc::new(ValidatedReceipt {
            receipt,
            scope,
            _reservation: Some(Arc::new(reservation)),
            failed,
        }))
    }
}

fn read_digest(
    view: &mut VerifiedView<'_>,
    demand: &ReadDemand,
    cancel: &AtomicBool,
    counter: &AtomicU64,
) -> io::Result<(String, u64)> {
    let mut digest = Sha256::new();
    let mut count_total = 0;
    let mut bytes = vec![0; 1024 * 1024];
    loop {
        let _cpu = super::work_admission::global_admission().acquire(
            demand.priority,
            cancel,
            demand.deadline_unix_ms,
        )?;
        let count = view.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        counter.fetch_add(count as u64, Ordering::AcqRel);
        count_total += count as u64;
        digest.update(&bytes[..count]);
    }
    Ok((hex::encode(digest.finalize()), count_total))
}

fn key(
    scope: &SourceReadScope,
    request: &VerificationRequest<'_>,
    attempt: &StructureAttemptRef,
) -> io::Result<String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "scope": scope.fingerprint(), "generation": scope.generation_identity(),
        "target": request.demand.target_fingerprint, "ranges": request.demand.ranges,
        "adapter": request.adapter_version, "policy": request.policy_version,
        "verification": request.demand.verification, "expected": request.expected_sha256,
        "attempt": attempt,
    }))
    .map_err(io::Error::other)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::other("verification lock poisoned")
}
