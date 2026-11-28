use std::collections::BTreeSet;

use domain::structure::{ActivityState, TargetFailure};
use serde::{Deserialize, Deserializer, Serialize};

use super::invalid;
use crate::DbResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ActivityRecord {
    pub case_id: String,
    pub run_id: String,
    pub work_key: String,
    pub attempt_id: String,
    pub owner_id: String,
    pub definition_sha256: String,
    pub input_capability_ids: BTreeSet<String>,
    pub target_id: String,
    pub state: ActivityState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_receipt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<TargetFailure>,
}

impl ActivityRecord {
    pub fn validate(&self) -> DbResult<()> {
        validate_key(&self.case_id, &self.run_id, &self.work_key)?;
        for (value, field) in [
            (self.attempt_id.as_str(), "attempt_id"),
            (self.owner_id.as_str(), "owner_id"),
            (self.target_id.as_str(), "target_id"),
        ] {
            require_text(value, field)?;
        }
        if self.definition_sha256.len() != 64
            || !self
                .definition_sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(invalid("activity definition digest is invalid"));
        }
        if self.input_capability_ids.is_empty() {
            return Err(invalid("activity input capability references are empty"));
        }
        for reference in &self.input_capability_ids {
            require_text(reference, "input_capability_id")?;
        }
        validate_outcome(
            self.state,
            self.output_receipt.as_deref(),
            self.failure.as_ref(),
        )
    }
}

impl<'de> Deserialize<'de> for ActivityRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            remote = "ActivityRecord",
            rename_all = "camelCase",
            deny_unknown_fields
        )]
        struct Record {
            case_id: String,
            run_id: String,
            work_key: String,
            attempt_id: String,
            owner_id: String,
            definition_sha256: String,
            input_capability_ids: BTreeSet<String>,
            target_id: String,
            state: ActivityState,
            output_receipt: Option<String>,
            failure: Option<TargetFailure>,
        }
        let record = Record::deserialize(deserializer)?;
        record.validate().map_err(serde::de::Error::custom)?;
        Ok(record)
    }
}

pub(super) fn validate_key(case_id: &str, run_id: &str, work_key: &str) -> DbResult<()> {
    require_text(case_id, "case_id")?;
    require_text(run_id, "run_id")?;
    require_text(work_key, "work_key")
}

pub(super) fn validate_outcome(
    state: ActivityState,
    receipt: Option<&str>,
    failure: Option<&TargetFailure>,
) -> DbResult<()> {
    if let Some(receipt) = receipt {
        require_text(receipt, "output_receipt")?;
    }
    if let Some(failure) = failure {
        failure
            .validate()
            .map_err(|_| invalid("activity failure reason is invalid"))?;
    }
    let valid = matches!(
        (state, receipt, failure),
        (
            ActivityState::Pending
                | ActivityState::Ready
                | ActivityState::Admitted
                | ActivityState::Running,
            None,
            None,
        ) | (ActivityState::Succeeded, Some(_), None)
            | (
                ActivityState::Blocked,
                None,
                Some(
                    TargetFailure::MissingCapabilities { .. }
                        | TargetFailure::MissingEvidence { .. }
                        | TargetFailure::RequiredDependencyFailed,
                ),
            )
            | (
                ActivityState::Failed,
                None,
                Some(
                    TargetFailure::UnsupportedFeature { .. }
                        | TargetFailure::CorruptMetadata { .. }
                        | TargetFailure::Conflicted { .. }
                        | TargetFailure::ResourceLimit { .. }
                        | TargetFailure::IoUnavailable
                        | TargetFailure::DeadlineExceeded,
                ),
            )
            | (
                ActivityState::Cancelled,
                None,
                Some(TargetFailure::Cancelled { .. })
            )
    );
    if !valid {
        return Err(invalid(
            "activity state disagrees with its receipt or failure reason",
        ));
    }
    Ok(())
}

pub(super) fn require_text(value: &str, field: &'static str) -> DbResult<()> {
    if value.trim().is_empty() {
        return Err(invalid(&format!("activity field is empty: {field}")));
    }
    Ok(())
}
