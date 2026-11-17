use crate::types::{StegSeverity, StegWarning};

/// Adds stable-code warnings while de-duplicating by code and enforcing a cap.
pub(crate) struct WarningCollector {
    items: Vec<StegWarning>,
    max: usize,
}

impl WarningCollector {
    pub(crate) fn new(max: usize) -> Self {
        Self {
            items: Vec::new(),
            max,
        }
    }

    pub(crate) fn push(
        &mut self,
        code: &'static str,
        severity: StegSeverity,
        message: impl Into<String>,
    ) {
        if self.items.len() >= self.max || self.items.iter().any(|item| item.code == code) {
            return;
        }
        self.items.push(StegWarning {
            code,
            severity,
            message: message.into(),
        });
    }

    pub(crate) fn into_vec(self) -> Vec<StegWarning> {
        self.items
    }
}
