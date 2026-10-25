use std::{
    borrow::Cow,
    sync::{Arc, OnceLock},
};

mod storage;
use storage::SnapshotBytes;

use super::{
    browser::ProvenanceSpan,
    log_replay::{
        prepare_log_replay, HiveLog, LogReplayReport, LogSlot, RecoveryState, ReplayedHive,
    },
    reader::{BrowserError, HiveView},
    RegistryError,
};

const MAX_HIVE_BYTES: usize = 256 * 1024 * 1024;

/// Immutable input used by registry queries after optional page-log recovery.
pub struct RegistrySnapshot<'a> {
    bytes: SnapshotBytes<'a>,
    report: Option<LogReplayReport>,
    provenance: Vec<ProvenanceSpan>,
    view: OnceLock<Result<HiveView, BrowserError>>,
    allocation_map: Option<Arc<Vec<u64>>>,
}

impl<'a> RegistrySnapshot<'a> {
    pub fn prepare(
        bytes: &'a [u8],
        log1: Option<&[u8]>,
        log2: Option<&[u8]>,
    ) -> Result<Self, RegistryError> {
        Self::prepare_input(Cow::Borrowed(bytes), log1, log2)
    }

    pub fn from_owned(
        bytes: Vec<u8>,
        log1: Option<&[u8]>,
        log2: Option<&[u8]>,
    ) -> Result<RegistrySnapshot<'static>, RegistryError> {
        RegistrySnapshot::prepare_input(Cow::Owned(bytes), log1, log2)
    }

    fn prepare_input(
        bytes: Cow<'a, [u8]>,
        log1: Option<&[u8]>,
        log2: Option<&[u8]>,
    ) -> Result<Self, RegistryError> {
        if bytes.len() > MAX_HIVE_BYTES || !bytes.starts_with(b"regf") {
            return Err(RegistryError::InvalidHive("invalid hive input".into()));
        }
        let logs: Vec<_> = [(LogSlot::Log1, log1), (LogSlot::Log2, log2)]
            .into_iter()
            .filter_map(|(slot, bytes)| {
                bytes.filter(|data| !data.is_empty()).map(|data| HiveLog {
                    slot,
                    bytes: Some(data),
                })
            })
            .collect();
        if logs.is_empty() {
            return Ok(Self {
                bytes: bytes.into(),
                report: None,
                provenance: Vec::new(),
                view: OnceLock::new(),
                allocation_map: None,
            });
        }
        let plan = prepare_log_replay(&bytes, &logs, MAX_HIVE_BYTES)
            .map_err(|failure| RegistryError::Recovery(failure.error))?;
        if plan.report().state == RecoveryState::NotNeeded {
            return Ok(Self {
                bytes: bytes.into(),
                report: Some(plan.report().clone()),
                provenance: Vec::new(),
                view: OnceLock::new(),
                allocation_map: None,
            });
        }
        let mut copy = bytes.into_owned();
        copy.reserve(
            plan.required_capacity(copy.len())
                .saturating_sub(copy.len()),
        );
        let recovered = plan
            .apply(copy)
            .map_err(|failure| RegistryError::Recovery(failure.error))?;
        Ok(Self::from_replayed(recovered))
    }

    pub fn bytes(&self) -> &[u8] {
        self.bytes.bytes()
    }

    pub fn report(&self) -> Option<&LogReplayReport> {
        self.report.as_ref()
    }

    pub fn provenance(&self) -> &[ProvenanceSpan] {
        &self.provenance
    }

    pub fn view(&self) -> Result<&HiveView, RegistryError> {
        self.view
            .get_or_init(|| {
                HiveView::from_prepared(
                    self.bytes.shared(),
                    self.allocation_map.clone(),
                    self.provenance.clone(),
                )
            })
            .as_ref()
            .map_err(|error| RegistryError::from(*error))
    }

    pub fn from_replayed(recovered: ReplayedHive) -> RegistrySnapshot<'static> {
        RegistrySnapshot {
            bytes: SnapshotBytes::Shared(Arc::new(recovered.bytes)),
            report: Some(recovered.report),
            provenance: recovered.provenance,
            allocation_map: recovered.allocation_map,
            view: OnceLock::new(),
        }
    }

    pub fn into_view(self) -> Result<HiveView, BrowserError> {
        match self.view.into_inner() {
            Some(view) => view,
            None => HiveView::from_prepared(
                self.bytes.into_shared(),
                self.allocation_map,
                self.provenance,
            ),
        }
    }

    pub fn logs_applied(&self) -> bool {
        self.report
            .as_ref()
            .is_some_and(|report| report.applied_pages > 0)
    }

    pub fn into_owned(self) -> RegistrySnapshot<'static> {
        RegistrySnapshot {
            bytes: self.bytes.into_owned(),
            report: self.report,
            provenance: self.provenance,
            view: self.view,
            allocation_map: self.allocation_map,
        }
    }
}
