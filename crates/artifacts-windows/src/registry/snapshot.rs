use std::borrow::Cow;

use super::{
    browser::ProvenanceSpan,
    log_replay::{prepare_log_replay, HiveLog, LogReplayReport, LogSlot, RecoveryState},
    RegistryError,
};

const MAX_HIVE_BYTES: usize = 256 * 1024 * 1024;

/// Immutable input used by registry queries after optional page-log recovery.
pub struct RegistrySnapshot<'a> {
    bytes: Cow<'a, [u8]>,
    report: Option<LogReplayReport>,
    provenance: Vec<ProvenanceSpan>,
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
                bytes,
                report: None,
                provenance: Vec::new(),
            });
        }
        let plan = prepare_log_replay(&bytes, &logs, MAX_HIVE_BYTES)
            .map_err(|failure| RegistryError::Recovery(failure.error))?;
        if plan.report().state == RecoveryState::NotNeeded {
            return Ok(Self {
                bytes,
                report: Some(plan.report().clone()),
                provenance: Vec::new(),
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
        Ok(Self {
            bytes: Cow::Owned(recovered.bytes),
            report: Some(recovered.report),
            provenance: recovered.provenance,
        })
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn report(&self) -> Option<&LogReplayReport> {
        self.report.as_ref()
    }

    pub fn provenance(&self) -> &[ProvenanceSpan] {
        &self.provenance
    }

    pub fn logs_applied(&self) -> bool {
        self.report
            .as_ref()
            .is_some_and(|report| report.applied_pages > 0)
    }

    pub fn into_owned(self) -> RegistrySnapshot<'static> {
        RegistrySnapshot {
            bytes: Cow::Owned(self.bytes.into_owned()),
            report: self.report,
            provenance: self.provenance,
        }
    }
}
