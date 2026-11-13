//! Validated bounds for one Timeline projection, independent of IPC or parsers.

/// Estimated projection workspace reservation in KiB, not a SQLite cache setting or an RSS limit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineProjectionPolicy {
    pub workspace_kib: u32,
}

impl Default for TimelineProjectionPolicy {
    fn default() -> Self {
        Self {
            workspace_kib: 1_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("timeline projection workspace budget is outside its bounded range")]
pub struct TimelineProjectionPolicyError;

impl TimelineProjectionPolicy {
    pub fn reservation_bytes(self) -> Result<u64, TimelineProjectionPolicyError> {
        self.validate()?;
        Ok(u64::from(self.workspace_kib) * 1024)
    }

    pub fn validate(self) -> Result<(), TimelineProjectionPolicyError> {
        if !(128 * 1024..=1024 * 1024).contains(&self.workspace_kib) {
            return Err(TimelineProjectionPolicyError);
        }
        Ok(())
    }
}
