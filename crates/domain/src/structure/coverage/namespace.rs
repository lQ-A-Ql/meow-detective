use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

use super::{require_digest, require_text, CoverageContext, CoverageError};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum NamespacePageEnd {
    More { cursor: String },
    Eof { witness_sha256: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NamespacePage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_cursor: Option<String>,
    pub first_ordinal: u64,
    pub object_count: u64,
    pub page_sha256: String,
    pub verified: bool,
    pub end: NamespacePageEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NamespaceCoverage {
    pub context: CoverageContext,
    pub root_id: String,
    pub query_sha256: String,
    pub ordering_id: String,
    pub pages: Vec<NamespacePage>,
}

impl NamespaceCoverage {
    pub fn validate(&self) -> Result<(), CoverageError> {
        self.context.validate()?;
        require_text(&self.root_id, "root_id")?;
        require_digest(&self.query_sha256, "query_sha256")?;
        require_text(&self.ordering_id, "ordering_id")?;
        let mut next_cursor: Option<&str> = None;
        let mut cursors = BTreeSet::new();
        let mut ordinal = 0u64;
        for (index, page) in self.pages.iter().enumerate() {
            if page.input_cursor.as_deref() != next_cursor {
                return Err(CoverageError::InvalidCursorChain);
            }
            if page.first_ordinal != ordinal {
                return Err(CoverageError::InvalidPageCount);
            }
            ordinal = ordinal
                .checked_add(page.object_count)
                .ok_or(CoverageError::InvalidPageCount)?;
            require_digest(&page.page_sha256, "page_sha256")?;
            let last = index + 1 == self.pages.len();
            if !page.verified && !last {
                return Err(CoverageError::UnverifiedContinuation);
            }
            match &page.end {
                NamespacePageEnd::More { cursor } => {
                    require_text(cursor, "continuation")?;
                    if !cursors.insert(cursor.as_str()) {
                        return Err(CoverageError::InvalidCursorChain);
                    }
                    next_cursor = Some(cursor);
                }
                NamespacePageEnd::Eof { witness_sha256 } => {
                    if !last || !page.verified {
                        return Err(CoverageError::InvalidEof);
                    }
                    require_digest(witness_sha256, "eof_witness")?;
                }
            }
        }
        Ok(())
    }

    pub fn verified_objects(&self) -> Result<u64, CoverageError> {
        self.validate()?;
        Ok(self
            .pages
            .iter()
            .filter(|page| page.verified)
            .map(|page| page.object_count)
            .sum())
    }

    pub(super) fn has_verified_eof(&self) -> bool {
        self.pages
            .last()
            .is_some_and(|page| page.verified && matches!(page.end, NamespacePageEnd::Eof { .. }))
    }
}

impl<'de> Deserialize<'de> for NamespaceCoverage {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Record {
            context: CoverageContext,
            root_id: String,
            query_sha256: String,
            ordering_id: String,
            pages: Vec<NamespacePage>,
        }
        let record = Record::deserialize(deserializer)?;
        let value = Self {
            context: record.context,
            root_id: record.root_id,
            query_sha256: record.query_sha256,
            ordering_id: record.ordering_id,
            pages: record.pages,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
