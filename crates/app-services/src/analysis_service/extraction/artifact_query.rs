use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::error::AnalysisServiceError;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::AnalysisParseStatusDto;

pub(super) struct AnalysisArtifactRow {
    pub(super) id: String,
    pub(super) source_object_id: Option<String>,
    pub(super) extractor_id: Option<String>,
    pub(super) created_at: String,
    pub(super) attrs: BTreeMap<String, Value>,
}

pub(super) fn already_has_v1_artifacts(
    conn: &Connection,
    candidate: &EvidenceCandidate,
) -> Result<bool, AnalysisServiceError> {
    let families = match candidate.category.as_str() {
        "Registry" => &[
            "RegistryValue",
            "RegistrySamUser",
            "RegistryUserAssist",
            "RegistryHive",
            "RegistryNetworkAdapter",
            "RegistryNetworkProfile",
            "RegistryInstalledSoftware",
            "RegistrySystemService",
            "RegistryUsbDevice",
            "RegistryMountedDevice",
            "RegistryShutdownTime",
            "RegistryShimCache",
            "RegistryMachineRunKey",
            "RegistryWinlogonConfig",
            "RegistryLsaPackage",
            "RegistryOpenSaveMru",
            "RegistryLastVisitedMru",
            "RegistryRunMru",
            "RegistryShellbag",
            "RegistryMuiCache",
            "RegistryAmcacheApplication",
            "RegistryAmcacheApplicationFile",
            "RegistryAppCompatLayer",
            "RegistrySecurityPolicy",
            "RegistryLsaSecret",
            "RegistryCachedCredential",
        ][..],
        "BrowserHistory" => &[
            "BrowserHistory",
            "BrowserDownload",
            "BrowserCookie",
            "BrowserSessionTab",
            "BrowserPassword",
        ][..],
        "Email" => &["EmailMessage"][..],
        "EventLogs" => &[
            "EvtxBootShutdown",
            "EvtxSecurityEvent",
            "EvtxApplicationEvent",
        ][..],
        "LinuxArtifacts" => &[
            "LinuxJournal",
            "LinuxWtmp",
            "LinuxBashCommand",
            "LinuxAptEvent",
            "LinuxCronJob",
            "LinuxSudoEvent",
            "LinuxSystemConfig",
            "LinuxWebSite",
            "LinuxWebAccessLog",
            "LinuxWebErrorLog",
            "LinuxWebFinding",
            "LinuxMysqlConfig",
            "LinuxMysqlLogEntry",
            "LinuxMysqlFinding",
        ][..],
        _ => &[][..],
    };
    if families.is_empty() {
        return Ok(false);
    }
    let placeholders = (1..=families.len())
        .map(|index| format!("?{}", index + 1))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT COUNT(*) FROM artifacts
         WHERE source_object_id = ?1 AND artifact_type IN ({placeholders})"
    );
    let mut params_values: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(candidate.file_id.0.clone())];
    for family in families {
        params_values.push(Box::new((*family).to_string()));
    }
    let params_refs = params_values
        .iter()
        .map(|param| param.as_ref())
        .collect::<Vec<&dyn rusqlite::types::ToSql>>();
    let count: i64 = conn.query_row(&sql, params_refs.as_slice(), |row| row.get(0))?;
    Ok(count > 0)
}

pub(super) fn count_analysis_artifacts(conn: &Connection) -> Result<u64, AnalysisServiceError> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type IN ('RegistryValue', 'RegistrySamUser', 'RegistryUserAssist', 'RegistryHive', 'RegistryNetworkAdapter', 'RegistryNetworkProfile', 'RegistryInstalledSoftware', 'RegistrySystemService', 'RegistryUsbDevice', 'RegistryMountedDevice', 'RegistryShutdownTime', 'RegistryShimCache', 'RegistryMachineRunKey', 'RegistryWinlogonConfig', 'RegistryLsaPackage', 'RegistryOpenSaveMru', 'RegistryLastVisitedMru', 'RegistryRunMru', 'RegistryShellbag', 'RegistryMuiCache', 'RegistryAmcacheApplication', 'RegistryAmcacheApplicationFile', 'RegistryAppCompatLayer', 'RegistrySecurityPolicy', 'RegistryLsaSecret', 'RegistryCachedCredential', 'BrowserHistory', 'BrowserDownload', 'BrowserCookie', 'BrowserSessionTab', 'BrowserPassword', 'EmailMessage', 'EvtxBootShutdown', 'EvtxSecurityEvent', 'EvtxApplicationEvent', 'LinuxJournal', 'LinuxWtmp', 'LinuxBashCommand', 'LinuxAptEvent', 'LinuxCronJob', 'LinuxSudoEvent', 'LinuxSystemConfig', 'LinuxWebSite', 'LinuxWebAccessLog', 'LinuxWebErrorLog', 'LinuxWebFinding', 'LinuxMysqlConfig', 'LinuxMysqlLogEntry', 'LinuxMysqlFinding')",
            [],
            |row| row.get(0),
        )
        ?;
    Ok(count as u64)
}

pub(super) fn count_artifacts_by_type(
    conn: &Connection,
    artifact_type: &str,
) -> Result<u64, AnalysisServiceError> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM artifacts WHERE artifact_type = ?1",
        [artifact_type],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

pub(super) fn count_artifacts_by_family_prefix(
    conn: &Connection,
    prefix: &str,
) -> Result<u64, AnalysisServiceError> {
    let pattern = format!("{}%", prefix);
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM artifacts WHERE artifact_type LIKE ?1",
        [pattern],
        |row| row.get(0),
    )?;
    Ok(count as u64)
}

pub(super) fn query_artifact_rows(
    conn: &Connection,
    families: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    if families.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=families.len())
        .map(|index| format!("?{}", index))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type IN ({})
         ORDER BY created_at DESC, id ASC
         LIMIT ?{} OFFSET ?{}",
        placeholders,
        families.len() + 1,
        families.len() + 2
    );
    let mut params_values: Vec<Box<dyn rusqlite::types::ToSql>> = families
        .iter()
        .map(|family| Box::new((*family).to_string()) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    params_values.push(Box::new(limit as i64));
    params_values.push(Box::new(offset as i64));
    let params_refs = params_values
        .iter()
        .map(|param| param.as_ref())
        .collect::<Vec<&dyn rusqlite::types::ToSql>>();
    query_artifact_rows_with_statement(conn, &sql, params_refs.as_slice())
}

pub(super) fn query_evtx_artifact_rows(
    conn: &Connection,
    families: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    if families.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (1..=families.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let limit_param = families.len() + 1;
    let offset_param = families.len() + 2;
    let sql = format!(
        "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type IN ({placeholders})
         ORDER BY julianday(json_extract(attrs, '$.timestamp')) IS NULL ASC,
                  julianday(json_extract(attrs, '$.timestamp')) DESC,
                  CAST(json_extract(attrs, '$.recordId') AS INTEGER) DESC,
                  id ASC
         LIMIT ?{limit_param} OFFSET ?{offset_param}"
    );
    let mut params_values: Vec<Box<dyn rusqlite::types::ToSql>> = families
        .iter()
        .map(|family| Box::new((*family).to_string()) as Box<dyn rusqlite::types::ToSql>)
        .collect();
    params_values.push(Box::new(limit as i64));
    params_values.push(Box::new(offset as i64));
    let params_refs = params_values
        .iter()
        .map(|param| param.as_ref())
        .collect::<Vec<&dyn rusqlite::types::ToSql>>();
    query_artifact_rows_with_statement(conn, &sql, params_refs.as_slice())
}

pub(super) fn query_evtx_artifact_rows_by_kinds(
    conn: &Connection,
    artifact_type: &str,
    kinds: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    if kinds.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = (2..=kinds.len() + 1)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let limit_param = kinds.len() + 2;
    let offset_param = kinds.len() + 3;
    let sql = format!(
        "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type = ?1 AND json_extract(attrs, '$.kind') IN ({placeholders})
         ORDER BY julianday(json_extract(attrs, '$.timestamp')) IS NULL ASC,
                  julianday(json_extract(attrs, '$.timestamp')) DESC,
                  CAST(json_extract(attrs, '$.recordId') AS INTEGER) DESC,
                  id ASC
         LIMIT ?{limit_param} OFFSET ?{offset_param}"
    );
    let mut params_values: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(artifact_type.to_string())];
    for kind in kinds {
        params_values.push(Box::new((*kind).to_string()));
    }
    params_values.push(Box::new(limit as i64));
    params_values.push(Box::new(offset as i64));
    let params_refs = params_values
        .iter()
        .map(|param| param.as_ref())
        .collect::<Vec<&dyn rusqlite::types::ToSql>>();
    query_artifact_rows_with_statement(conn, &sql, params_refs.as_slice())
}

fn query_artifact_rows_with_statement(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::types::ToSql],
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params, |row| {
        let attrs_text: String = row.get(4)?;
        Ok(AnalysisArtifactRow {
            id: row.get(0)?,
            source_object_id: row.get(1)?,
            extractor_id: row.get(2)?,
            created_at: row.get(3)?,
            attrs: serde_json::from_str(&attrs_text).unwrap_or_default(),
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub(super) fn status_from_total(total: u64) -> AnalysisParseStatusDto {
    if total > 0 {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotFound
    }
}
