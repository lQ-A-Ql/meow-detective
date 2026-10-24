use crate::connection::DbResult;
use rusqlite::{params, Connection};

pub struct KubernetesAnalysisRepo<'a> {
    conn: &'a Connection,
}

pub struct KubernetesAnalysisRunRecord<'a> {
    pub id: &'a str,
    pub case_id: &'a str,
    pub import_set_id: &'a str,
    pub scope_id: &'a str,
    pub attempt: u32,
    pub expected_member_count: u32,
    pub started_at: &'a str,
}

pub struct KubernetesAnalysisArtifactRecord<'a> {
    pub run_id: &'a str,
    pub source_id: &'a str,
    pub file_id: &'a str,
    pub kind: &'a str,
    pub status: &'a str,
    pub diagnostics: &'a str,
    pub digest: Option<&'a str>,
}

impl<'a> KubernetesAnalysisRepo<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn next_attempt(
        &self,
        case_id: &str,
        import_set_id: &str,
        scope_id: &str,
    ) -> DbResult<u32> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(attempt), 0) + 1 FROM kubernetes_analysis_runs WHERE case_id = ?1 AND import_set_id = ?2 AND scope_id = ?3",
            params![case_id, import_set_id, scope_id], |row| row.get::<_, i64>(0))?.max(1) as u32)
    }

    pub fn insert_run(&self, record: KubernetesAnalysisRunRecord<'_>) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO kubernetes_analysis_runs (id, case_id, import_set_id, scope_id, state, attempt, expected_member_count, started_at) VALUES (?1, ?2, ?3, ?4, 'running', ?5, ?6, ?7)",
            params![record.id, record.case_id, record.import_set_id, record.scope_id, record.attempt, record.expected_member_count, record.started_at])?;
        Ok(())
    }

    pub fn replace_artifact(&self, record: KubernetesAnalysisArtifactRecord<'_>) -> DbResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO kubernetes_analysis_artifacts (run_id, data_source_id, file_id, kind, status, diagnostics_json, content_digest) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![record.run_id, record.source_id, record.file_id, record.kind, record.status, record.diagnostics, record.digest])?;
        Ok(())
    }

    pub fn finish(
        &self,
        id: &str,
        state: &str,
        parsed: u32,
        failed: u32,
        diagnostics: &str,
        finished_at: &str,
    ) -> DbResult<()> {
        self.conn.execute(
            "UPDATE kubernetes_analysis_runs SET state = ?1, parsed_artifact_count = ?2, failed_artifact_count = ?3, diagnostics_json = ?4, finished_at = ?5 WHERE id = ?6",
            params![state, parsed, failed, diagnostics, finished_at, id])?;
        Ok(())
    }

    pub fn mark_terminal(
        &self,
        id: &str,
        state: &str,
        diagnostics: &str,
        finished_at: &str,
    ) -> DbResult<()> {
        self.conn.execute(
            "UPDATE kubernetes_analysis_runs SET state = ?1, diagnostics_json = ?2, finished_at = ?3 WHERE id = ?4",
            params![state, diagnostics, finished_at, id])?;
        Ok(())
    }
}
