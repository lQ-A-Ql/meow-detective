use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct CaseId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseMeta {
    pub id: CaseId,
    pub name: String,
    pub number: Option<String>,
    pub examiner: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Domain behavior for CaseMeta
impl CaseMeta {
    /// Check if this case was recently active (within the last 24 hours).
    pub fn is_active(&self) -> bool {
        self.updated_at > Utc::now() - chrono::Duration::hours(24)
    }

    /// Get the display name for this case.
    ///
    /// If a case number is set, returns `[number] name`.
    /// Otherwise, returns just the name.
    pub fn display_name(&self) -> String {
        match &self.number {
            Some(num) => format!("[{}] {}", num, self.name),
            None => self.name.clone(),
        }
    }

    /// Get a short summary of the case.
    pub fn summary(&self) -> String {
        let mut parts = vec![self.name.clone()];
        if let Some(ref examiner) = self.examiner {
            parts.push(format!("by {}", examiner));
        }
        parts.join(" ")
    }

    /// Check if the case has an examiner assigned.
    pub fn has_examiner(&self) -> bool {
        self.examiner.as_ref().is_some_and(|e| !e.is_empty())
    }
}

#[derive(Debug, Clone)]
pub struct CaseSession {
    pub case_id: CaseId,
    pub case_root: PathBuf,
    pub opened_at: DateTime<Utc>,
}

/// Domain behavior for CaseSession
impl CaseSession {
    /// Get the path to the case database file.
    pub fn db_path(&self) -> PathBuf {
        self.case_root.join("forensics.db")
    }

    /// Get the path to the case indexes directory.
    pub fn indexes_path(&self) -> PathBuf {
        self.case_root.join("indexes")
    }
}

#[cfg(test)]
#[path = "../../tests/unit/case.rs"]
mod tests;
