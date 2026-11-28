use serde::{Deserialize, Deserializer, Serialize};

use super::invalid;
use crate::DbResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamespaceCheckpointRecord {
    pub case_id: String,
    pub run_id: String,
    pub target_id: String,
    pub snapshot_id: String,
    pub query_sha256: String,
    pub ordering_id: String,
    pub attempt_id: String,
    pub owner_id: String,
    pub next_page: u64,
    pub next_ordinal: u64,
    pub verified_objects: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub eof: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_page_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_receipt: Option<String>,
}

impl NamespaceCheckpointRecord {
    pub fn validate(&self) -> DbResult<()> {
        for (value, field) in [
            (self.case_id.as_str(), "case_id"),
            (self.run_id.as_str(), "run_id"),
            (self.target_id.as_str(), "target_id"),
            (self.snapshot_id.as_str(), "snapshot_id"),
            (self.ordering_id.as_str(), "ordering_id"),
            (self.attempt_id.as_str(), "attempt_id"),
            (self.owner_id.as_str(), "owner_id"),
        ] {
            require_text(value, field)?;
        }
        require_digest(&self.query_sha256)?;
        if self.verified_objects != self.next_ordinal {
            return Err(invalid("checkpoint object counts disagree"));
        }
        if self.next_page == 0 {
            if self.next_ordinal != 0
                || self.next_cursor.is_some()
                || self.eof
                || self.last_page_sha256.is_some()
                || self.output_receipt.is_some()
            {
                return Err(invalid("new checkpoint contains committed page state"));
            }
            return Ok(());
        }
        require_digest(
            self.last_page_sha256
                .as_deref()
                .ok_or_else(|| invalid("checkpoint last page digest is missing"))?,
        )?;
        require_text(
            self.output_receipt
                .as_deref()
                .ok_or_else(|| invalid("checkpoint output receipt is missing"))?,
            "output_receipt",
        )?;
        validate_cursor(self.eof, self.next_cursor.as_deref())?;
        if !self.eof && self.next_ordinal == 0 {
            return Err(invalid(
                "a nonterminal checkpoint must make object progress",
            ));
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for NamespaceCheckpointRecord {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            remote = "NamespaceCheckpointRecord",
            rename_all = "camelCase",
            deny_unknown_fields
        )]
        struct Record {
            case_id: String,
            run_id: String,
            target_id: String,
            snapshot_id: String,
            query_sha256: String,
            ordering_id: String,
            attempt_id: String,
            owner_id: String,
            next_page: u64,
            next_ordinal: u64,
            verified_objects: u64,
            next_cursor: Option<String>,
            eof: bool,
            last_page_sha256: Option<String>,
            output_receipt: Option<String>,
        }
        let record = Record::deserialize(deserializer)?;
        record.validate().map_err(serde::de::Error::custom)?;
        Ok(record)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamespacePageCommit {
    pub first_ordinal: u64,
    pub object_count: u64,
    pub page_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub eof: bool,
    pub output_receipt: String,
}

impl NamespacePageCommit {
    pub fn validate(&self) -> DbResult<()> {
        require_digest(&self.page_sha256)?;
        require_text(&self.output_receipt, "output_receipt")?;
        validate_cursor(self.eof, self.next_cursor.as_deref())?;
        self.first_ordinal
            .checked_add(self.object_count)
            .ok_or_else(|| invalid("checkpoint object ordinal overflow"))?;
        if !self.eof && self.object_count == 0 {
            return Err(invalid("an empty namespace page must be terminal"));
        }
        Ok(())
    }

    pub(super) fn identity(&self, page: u64) -> DbResult<String> {
        let identity =
            domain::structure::canonical_identity("namespace-page-commit-v1", &(page, self))
                .map_err(|_| invalid("checkpoint page identity cannot be serialized"))?;
        Ok(identity)
    }
}

impl<'de> Deserialize<'de> for NamespacePageCommit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(
            remote = "NamespacePageCommit",
            rename_all = "camelCase",
            deny_unknown_fields
        )]
        struct Record {
            first_ordinal: u64,
            object_count: u64,
            page_sha256: String,
            next_cursor: Option<String>,
            eof: bool,
            output_receipt: String,
        }
        let page = Record::deserialize(deserializer)?;
        page.validate().map_err(serde::de::Error::custom)?;
        Ok(page)
    }
}

pub(super) fn require_text(value: &str, field: &str) -> DbResult<()> {
    if value.trim().is_empty() {
        return Err(invalid(&format!("checkpoint field is empty: {field}")));
    }
    Ok(())
}

fn require_digest(value: &str) -> DbResult<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(invalid("checkpoint digest must be lowercase SHA-256"));
    }
    Ok(())
}

fn validate_cursor(eof: bool, cursor: Option<&str>) -> DbResult<()> {
    match (eof, cursor) {
        (true, None) => Ok(()),
        (false, Some(cursor)) => require_text(cursor, "next_cursor"),
        _ => Err(invalid("checkpoint cursor and EOF disagree")),
    }
}
