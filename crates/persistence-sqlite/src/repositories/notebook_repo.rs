use crate::connection::DbResult;
use crate::sql_builder::{placeholders, ClauseBuilder};
use domain::{EntryStatus, EvidenceCitation, NodeType, NotebookEntry, NotebookEntryType};
use rusqlite::{params, Connection};
use std::collections::HashMap;

/// A recorded investigation step for provenance tracking.
#[derive(Debug, Clone, PartialEq)]
pub struct InvestigationStep {
    pub id: String,
    pub case_id: String,
    pub step_kind: String,
    pub params_json: String,
    pub timestamp: String,
    pub duration_ms: Option<i64>,
    pub case_state_hash: Option<String>,
    pub success: Option<bool>,
    pub error_code: Option<String>,
}

/// Filter criteria for listing notebook entries.
#[derive(Debug, Clone, Default)]
pub struct NotebookEntryFilters {
    pub entry_type: Option<NotebookEntryType>,
    pub status: Option<EntryStatus>,
    pub tags: Option<Vec<String>>,
    pub search: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Filter criteria for listing investigation steps.
#[derive(Debug, Clone, Default)]
pub struct StepFilters {
    pub step_kind: Option<String>,
    pub success: Option<bool>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

// ── SQL constants ──

const NOTEBOOK_ENTRY_COLUMNS: &str =
    "id, case_id, parent_id, author, entry_type, title, body_markdown, tags, status, created_at, updated_at";

const CITATION_COLUMNS: &str =
    "id, entry_id, target_node_type, target_node_id, display_label, snippet, cited_at";

const STEP_COLUMNS: &str =
    "id, case_id, step_kind, params_json, timestamp, duration_ms, case_state_hash, success, error_code";

pub struct NotebookRepo<'a> {
    conn: &'a Connection,
}

impl<'a> NotebookRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    // ── Notebook entries ──

    /// Insert a new notebook entry. Returns an error if the id already exists.
    pub fn create_entry(&self, entry: &NotebookEntry) -> DbResult<()> {
        let sql = format!(
            "INSERT INTO notebook_entries ({NOTEBOOK_ENTRY_COLUMNS})
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
        );
        let tags_json = serde_json::to_string(&entry.tags).unwrap_or_default();
        self.conn.execute(
            &sql,
            params![
                entry.id,
                entry.case_id,
                entry.parent_id,
                entry.author,
                entry_type_str(&entry.entry_type),
                entry.title,
                entry.body_markdown,
                tags_json,
                entry_status_str(&entry.status),
                entry.created_at,
                entry.updated_at,
            ],
        )?;
        Ok(())
    }

    /// Update an existing notebook entry. Only updates fields that are provided (non-None in update).
    /// For simple partial updates, overwrites title, body_markdown, tags, status, updated_at.
    pub fn update_entry(
        &self,
        id: &str,
        title: Option<&str>,
        body_markdown: Option<&str>,
        tags: Option<&[String]>,
        status: Option<&EntryStatus>,
        updated_at: &str,
    ) -> DbResult<()> {
        // Build a dynamic UPDATE only touching fields that are provided.
        let mut builder = ClauseBuilder::new();
        if let Some(t) = title {
            builder.push_eq("title", t.to_string());
        }
        if let Some(b) = body_markdown {
            builder.push_eq("body_markdown", b.to_string());
        }
        if let Some(t) = tags {
            builder.push_eq("tags", serde_json::to_string(t).unwrap_or_default());
        }
        if let Some(s) = status {
            builder.push_eq("status", entry_status_str(s).to_string());
        }
        builder.push_eq("updated_at", updated_at.to_string());
        let id_param = builder.push_param(id.to_string());

        let sql = format!(
            "UPDATE notebook_entries SET {} WHERE id = ?{id_param}",
            builder.set_clause(),
        );
        self.conn.execute(&sql, builder.param_refs().as_slice())?;
        Ok(())
    }

    /// Retrieve a single notebook entry by id.
    pub fn get_entry(&self, id: &str) -> DbResult<Option<NotebookEntry>> {
        let sql = format!("SELECT {NOTEBOOK_ENTRY_COLUMNS} FROM notebook_entries WHERE id = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let result = stmt.query_row(params![id], row_to_notebook_entry);
        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// List notebook entries for a case with optional filters, ordered by created_at descending.
    pub fn list_entries(
        &self,
        case_id: &str,
        filters: &NotebookEntryFilters,
    ) -> DbResult<Vec<NotebookEntry>> {
        let mut builder = ClauseBuilder::new();
        builder.push_eq("case_id", case_id.to_string());

        if let Some(ref et) = filters.entry_type {
            builder.push_eq("entry_type", entry_type_str(et).to_string());
        }
        if let Some(ref s) = filters.status {
            builder.push_eq("status", entry_status_str(s).to_string());
        }
        if let Some(ref search) = filters.search {
            let next_param = builder.next_param();
            let pattern = format!("%{search}%");
            builder.push_raw(
                format!(
                    "(title LIKE ?{next_param} OR body_markdown LIKE ?{})",
                    next_param + 1
                ),
                vec![pattern.clone(), pattern],
            );
        }
        // tag filter uses JSON array LIKE
        if let Some(ref tags) = filters.tags {
            for tag in tags {
                builder.push_cmp("tags", "LIKE", format!("%\"{tag}\"%"));
            }
        }

        let limit = filters.limit.unwrap_or(500);
        let offset = filters.offset.unwrap_or(0);
        let limit_param = builder.push_param(limit);
        let offset_param = builder.push_param(offset);

        let sql = format!(
            "SELECT {NOTEBOOK_ENTRY_COLUMNS} FROM notebook_entries
             {}
             ORDER BY created_at DESC
             LIMIT ?{limit_param} OFFSET ?{offset_param}",
            builder.where_clause(),
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(builder.param_refs().as_slice(), row_to_notebook_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Retrieve a thread of notebook entries starting from a root id, using a recursive CTE.
    /// Returns entries in depth-first order (parent before children).
    pub fn get_thread(&self, root_id: &str) -> DbResult<Vec<NotebookEntry>> {
        let sql = format!(
            "WITH RECURSIVE thread AS (
                SELECT {NOTEBOOK_ENTRY_COLUMNS}, 0 AS depth
                FROM notebook_entries
                WHERE id = ?1
                UNION ALL
                SELECT ne.{cols}, t.depth + 1
                FROM notebook_entries ne
                JOIN thread t ON ne.parent_id = t.id
            )
            SELECT {NOTEBOOK_ENTRY_COLUMNS}
            FROM thread
            ORDER BY depth ASC, created_at ASC",
            cols = NOTEBOOK_ENTRY_COLUMNS.replace(", ", ", ne."),
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![root_id], row_to_notebook_entry)?;
        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }
        Ok(entries)
    }

    /// Soft-delete a notebook entry by clearing its body and marking status as 'deleted'.
    pub fn delete_entry(&self, id: &str, updated_at: &str) -> DbResult<()> {
        self.conn.execute(
            "UPDATE notebook_entries SET status = 'deleted', body_markdown = '', updated_at = ?1 WHERE id = ?2",
            params![updated_at, id],
        )?;
        Ok(())
    }

    /// Hard-delete all notebook entries for a case (used during case deletion).
    pub fn delete_case_notebook(&self, case_id: &str) -> DbResult<()> {
        // Evidence citations cascade-delete with their entry, but delete them explicitly first
        self.conn.execute(
            "DELETE FROM evidence_citations WHERE entry_id IN (SELECT id FROM notebook_entries WHERE case_id = ?1)",
            params![case_id],
        )?;
        self.conn.execute(
            "DELETE FROM notebook_entries WHERE case_id = ?1",
            params![case_id],
        )?;
        Ok(())
    }

    // ── Evidence citations ──

    /// Add a citation linking a notebook entry to a graph node.
    pub fn add_citation(&self, citation: &EvidenceCitation) -> DbResult<()> {
        let sql = format!(
            "INSERT INTO evidence_citations ({CITATION_COLUMNS})
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"
        );
        self.conn.execute(
            &sql,
            params![
                citation.id,
                citation.entry_id,
                node_type_str(&citation.target_node_type),
                citation.target_node_id,
                citation.display_label,
                citation.snippet,
                citation.cited_at,
            ],
        )?;
        Ok(())
    }

    /// List all citations for a given notebook entry.
    pub fn list_citations_for_entry(&self, entry_id: &str) -> DbResult<Vec<EvidenceCitation>> {
        let sql = format!(
            "SELECT {CITATION_COLUMNS} FROM evidence_citations WHERE entry_id = ?1 ORDER BY cited_at ASC"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![entry_id], row_to_citation)?;
        let mut citations = Vec::new();
        for row in rows {
            citations.push(row?);
        }
        Ok(citations)
    }

    /// Count non-deleted notebook entries belonging to a case.
    pub fn count_active_entries_for_case(&self, case_id: &str) -> DbResult<u32> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM notebook_entries WHERE case_id = ?1 AND status != 'deleted'",
                params![case_id],
                |row| row.get::<_, i64>(0).map(|count| count.max(0) as u32),
            )
            .map_err(Into::into)
    }

    /// Count citations attached to non-deleted notebook entries in a case.
    pub fn count_citations_for_case(&self, case_id: &str) -> DbResult<u32> {
        self.conn
            .query_row(
                "SELECT COUNT(*) FROM evidence_citations
                 WHERE entry_id IN (
                     SELECT id FROM notebook_entries
                     WHERE case_id = ?1 AND status != 'deleted'
                 )",
                params![case_id],
                |row| row.get::<_, i64>(0).map(|count| count.max(0) as u32),
            )
            .map_err(Into::into)
    }

    /// Remove a citation by id.
    pub fn remove_citation(&self, citation_id: &str) -> DbResult<()> {
        self.conn.execute(
            "DELETE FROM evidence_citations WHERE id = ?1",
            params![citation_id],
        )?;
        Ok(())
    }

    /// Batch citations by entry id for efficient lookups.
    /// Returns a map from entry_id to Vec<EvidenceCitation>.
    pub fn batch_citations_for_entries(
        &self,
        entry_ids: &[String],
    ) -> DbResult<HashMap<String, Vec<EvidenceCitation>>> {
        if entry_ids.is_empty() {
            return Ok(HashMap::new());
        }

        // Dynamic IN clause
        let sql = format!(
            "SELECT {CITATION_COLUMNS} FROM evidence_citations
             WHERE entry_id IN ({}) ORDER BY cited_at ASC",
            placeholders(1, entry_ids.len()),
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let string_refs: Vec<&str> = entry_ids.iter().map(|s| s.as_str()).collect();
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = string_refs
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();

        let rows = stmt.query_map(param_refs.as_slice(), row_to_citation)?;
        let mut map: HashMap<String, Vec<EvidenceCitation>> = HashMap::new();
        for row in rows {
            let citation = row?;
            map.entry(citation.entry_id.clone())
                .or_default()
                .push(citation);
        }
        Ok(map)
    }

    // ── Investigation steps ──

    /// Record an investigation step.
    pub fn record_step(&self, step: &InvestigationStep) -> DbResult<()> {
        let sql = format!(
            "INSERT INTO investigation_steps ({STEP_COLUMNS})
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        );
        self.conn.execute(
            &sql,
            params![
                step.id,
                step.case_id,
                step.step_kind,
                step.params_json,
                step.timestamp,
                step.duration_ms,
                step.case_state_hash,
                step.success.map(|b| b as i32),
                step.error_code,
            ],
        )?;
        Ok(())
    }

    /// List investigation steps for a case with optional filters, ordered by timestamp descending.
    pub fn list_steps(
        &self,
        case_id: &str,
        filters: &StepFilters,
    ) -> DbResult<Vec<InvestigationStep>> {
        let mut builder = ClauseBuilder::new();
        builder.push_eq("case_id", case_id.to_string());

        if let Some(ref kind) = filters.step_kind {
            builder.push_eq("step_kind", kind.to_string());
        }
        if let Some(success_val) = filters.success {
            builder.push_eq("success", success_val as i32);
        }

        let limit = filters.limit.unwrap_or(500);
        let offset = filters.offset.unwrap_or(0);
        let limit_param = builder.push_param(limit);
        let offset_param = builder.push_param(offset);

        let sql = format!(
            "SELECT {STEP_COLUMNS} FROM investigation_steps
             {}
             ORDER BY timestamp DESC
             LIMIT ?{limit_param} OFFSET ?{offset_param}",
            builder.where_clause(),
        );

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(builder.param_refs().as_slice(), row_to_step)?;
        let mut steps = Vec::new();
        for row in rows {
            steps.push(row?);
        }
        Ok(steps)
    }
}

// ── Serialization helpers ──

fn entry_type_str(et: &NotebookEntryType) -> &'static str {
    match et {
        NotebookEntryType::Observation => "observation",
        NotebookEntryType::Hypothesis => "hypothesis",
        NotebookEntryType::Finding => "finding",
        NotebookEntryType::ActionItem => "action_item",
        NotebookEntryType::Conclusion => "conclusion",
    }
}

fn parse_entry_type(s: &str) -> NotebookEntryType {
    match s {
        "observation" => NotebookEntryType::Observation,
        "hypothesis" => NotebookEntryType::Hypothesis,
        "finding" => NotebookEntryType::Finding,
        "action_item" => NotebookEntryType::ActionItem,
        "conclusion" => NotebookEntryType::Conclusion,
        _ => NotebookEntryType::Observation,
    }
}

fn entry_status_str(status: &EntryStatus) -> &'static str {
    match status {
        EntryStatus::Draft => "draft",
        EntryStatus::Reviewed => "reviewed",
        EntryStatus::Final => "final",
    }
}

fn parse_entry_status(s: &str) -> EntryStatus {
    match s {
        "draft" => EntryStatus::Draft,
        "reviewed" => EntryStatus::Reviewed,
        "final" => EntryStatus::Final,
        "deleted" => EntryStatus::Draft, // soft-deleted entries: treat as Draft
        _ => EntryStatus::Draft,
    }
}

fn node_type_str(nt: &NodeType) -> &'static str {
    match nt {
        NodeType::File => "file",
        NodeType::Artifact => "artifact",
        NodeType::TimelineEvent => "timeline_event",
        NodeType::Entity => "entity",
        NodeType::Lead => "lead",
        NodeType::NotebookEntry => "notebook_entry",
    }
}

fn parse_node_type(s: &str) -> NodeType {
    match s {
        "file" => NodeType::File,
        "artifact" => NodeType::Artifact,
        "timeline_event" => NodeType::TimelineEvent,
        "entity" => NodeType::Entity,
        "lead" => NodeType::Lead,
        "notebook_entry" => NodeType::NotebookEntry,
        _ => NodeType::Entity,
    }
}

// ── Row mappers ──

fn row_to_notebook_entry(row: &rusqlite::Row) -> rusqlite::Result<NotebookEntry> {
    let tags_str: String = row.get(7)?;
    let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
    Ok(NotebookEntry {
        id: row.get(0)?,
        case_id: row.get(1)?,
        parent_id: row.get(2)?,
        author: row.get(3)?,
        entry_type: parse_entry_type(&row.get::<_, String>(4)?),
        title: row.get(5)?,
        body_markdown: row.get(6)?,
        tags,
        status: parse_entry_status(&row.get::<_, String>(8)?),
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn row_to_citation(row: &rusqlite::Row) -> rusqlite::Result<EvidenceCitation> {
    Ok(EvidenceCitation {
        id: row.get(0)?,
        entry_id: row.get(1)?,
        target_node_type: parse_node_type(&row.get::<_, String>(2)?),
        target_node_id: row.get(3)?,
        display_label: row.get(4)?,
        snippet: row.get(5)?,
        cited_at: row.get(6)?,
    })
}

fn row_to_step(row: &rusqlite::Row) -> rusqlite::Result<InvestigationStep> {
    Ok(InvestigationStep {
        id: row.get(0)?,
        case_id: row.get(1)?,
        step_kind: row.get(2)?,
        params_json: row.get(3)?,
        timestamp: row.get(4)?,
        duration_ms: row.get(5)?,
        case_state_hash: row.get(6)?,
        success: row.get::<_, Option<i32>>(7)?.map(|v| v != 0),
        error_code: row.get(8)?,
    })
}

// ── Tests ──

#[cfg(test)]
#[path = "../../tests/unit/repositories/notebook_repo.rs"]
mod tests;
