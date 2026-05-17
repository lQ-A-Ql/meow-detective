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

#[derive(Debug, Clone)]
pub struct CaseSession {
    pub case_id: CaseId,
    pub case_root: PathBuf,
    pub opened_at: DateTime<Utc>,
}
