use crate::analysis_service::candidates::EvidenceCandidate;
use crate::analysis_service::error::AnalysisServiceError;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::BTreeMap;
use transport::dto::AnalysisParseStatusDto;
mod linux;

pub(super) use linux::{query_linux_kernel_image_versions, query_linux_module_dir_versions};

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
            "RegistryPortableDevice",
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
            "BrowserExtension",
        ][..],
        "Email" => &["EmailMessage", "EmailAccount"][..],
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
    let records = artifact_records(conn)?;
    Ok(records.iter().any(|record| {
        record.source_object_id.as_deref() == Some(candidate.file_id.0.as_str())
            && families.contains(&record.artifact_type.as_str())
            && (candidate.category != "Registry"
                || record.attrs.get("extractorVersion").and_then(Value::as_str)
                    == Some(crate::analysis_service::capability::REGISTRY_EXTRACTOR_VERSION))
    }))
}

pub(super) fn count_analysis_artifacts(conn: &Connection) -> Result<u64, AnalysisServiceError> {
    Ok(artifact_records(conn)?
        .iter()
        .filter(|record| analysis_families().contains(&record.artifact_type.as_str()))
        .count() as u64)
}

pub(super) fn count_artifacts_by_type(
    conn: &Connection,
    artifact_type: &str,
) -> Result<u64, AnalysisServiceError> {
    Ok(artifact_records(conn)?
        .iter()
        .filter(|record| record.artifact_type == artifact_type)
        .count() as u64)
}

/// Count artifacts of one type produced by a specific extractor. Used to keep
/// the journal summary count limited to real systemd journal entries instead
/// of the generic text-log fallback records that share the `LinuxJournal`
/// artifact type.
pub(super) fn count_artifacts_by_type_and_extractor(
    conn: &Connection,
    artifact_type: &str,
    extractor_id: &str,
) -> Result<u64, AnalysisServiceError> {
    Ok(artifact_records(conn)?
        .iter()
        .filter(|record| {
            record.artifact_type == artifact_type
                && record.attrs.get("extractorId").and_then(Value::as_str) == Some(extractor_id)
        })
        .count() as u64)
}

pub(super) fn count_artifacts_by_type_excluding_extractor(
    conn: &Connection,
    artifact_type: &str,
    extractor_id: &str,
) -> Result<u64, AnalysisServiceError> {
    Ok(artifact_records(conn)?
        .iter()
        .filter(|record| {
            record.artifact_type == artifact_type
                && record.attrs.get("extractorId").and_then(Value::as_str) != Some(extractor_id)
        })
        .count() as u64)
}

pub(super) fn count_artifacts_by_family_prefix(
    conn: &Connection,
    prefix: &str,
) -> Result<u64, AnalysisServiceError> {
    Ok(artifact_records(conn)?
        .iter()
        .filter(|record| record.artifact_type.starts_with(prefix))
        .count() as u64)
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
    let mut rows = artifact_records(conn)?
        .into_iter()
        .filter(|record| families.contains(&record.artifact_type.as_str()))
        .map(artifact_row)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .created_at
            .cmp(&left.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(rows
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect())
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
    let mut rows = artifact_records(conn)?
        .into_iter()
        .filter(|record| families.contains(&record.artifact_type.as_str()))
        .map(artifact_row)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        let left_ts = left
            .attrs
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("");
        let right_ts = right
            .attrs
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or("");
        right_ts.cmp(left_ts).then_with(|| left.id.cmp(&right.id))
    });
    Ok(rows
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect())
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
    let mut rows = artifact_records(conn)?
        .into_iter()
        .filter(|record| {
            record.artifact_type == artifact_type
                && record
                    .attrs
                    .get("kind")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| kinds.contains(&kind))
        })
        .map(artifact_row)
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| right.id.cmp(&left.id));
    Ok(rows
        .into_iter()
        .skip(offset as usize)
        .take(limit as usize)
        .collect())
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

/// Count `LinuxSystemConfig` rows of one `configKind` (passwdAccount,
/// shadowAccount, osRelease, ...). Used by the unpaged system-info overview.
/// Rows extracted from Docker overlay layers (`overlayContext`) describe
/// container content, not host state, and are excluded.
pub(super) fn count_linux_system_config_by_kind(
    conn: &Connection,
    config_kind: &str,
) -> Result<u64, AnalysisServiceError> {
    Ok(artifact_records(conn)?
        .iter()
        .filter(|record| {
            record.artifact_type == "LinuxSystemConfig"
                && record.attrs.get("configKind").and_then(Value::as_str) == Some(config_kind)
                && record.attrs.get("overlayContext").is_none()
        })
        .count() as u64)
}

/// Count passwd account rows whose uid is in the regular-user range.
pub(super) fn count_linux_user_accounts(conn: &Connection) -> Result<u64, AnalysisServiceError> {
    Ok(artifact_records(conn)?
        .iter()
        .filter(|record| {
            record.artifact_type == "LinuxSystemConfig"
                && record.attrs.get("configKind").and_then(Value::as_str) == Some("passwdAccount")
                && record
                    .attrs
                    .get("uid")
                    .and_then(Value::as_i64)
                    .is_some_and(|uid| uid >= 1000)
                && record.attrs.get("overlayContext").is_none()
        })
        .count() as u64)
}

/// Count shadow account rows flagged `locked`.
pub(super) fn count_linux_locked_shadow_accounts(
    conn: &Connection,
) -> Result<u64, AnalysisServiceError> {
    Ok(artifact_records(conn)?
        .iter()
        .filter(|record| {
            record.artifact_type == "LinuxSystemConfig"
                && record.attrs.get("configKind").and_then(Value::as_str) == Some("shadowAccount")
                && record
                    .attrs
                    .get("locked")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                && record.attrs.get("overlayContext").is_none()
        })
        .count() as u64)
}

/// Fetch `LinuxSystemConfig` rows of one `configKind` without paging. The
/// system-info overview needs them independent of the summary entry window.
/// Docker overlay rows are excluded (container content, not host state).
pub(super) fn query_linux_system_config_by_kind(
    conn: &Connection,
    config_kind: &str,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    return Ok(artifact_records(conn)?
        .into_iter()
        .filter(|record| {
            record.artifact_type == "LinuxSystemConfig"
                && record.attrs.get("configKind").and_then(Value::as_str) == Some(config_kind)
                && record.attrs.get("overlayContext").is_none()
        })
        .take(limit as usize)
        .map(artifact_row)
        .collect());
    #[allow(unreachable_code)]
    let sql = "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type = 'LinuxSystemConfig'
           AND json_extract(attrs, '$.configKind') = ?1
           AND json_extract(attrs, '$.overlayContext') IS NULL
         ORDER BY id ASC
         LIMIT ?2";
    query_artifact_rows_with_statement(
        conn,
        sql,
        &[&config_kind as &dyn rusqlite::types::ToSql, &(limit as i64)],
    )
}

pub(super) fn count_linux_system_config_by_kind_paged(
    conn: &Connection,
    config_kind: &str,
) -> Result<u64, AnalysisServiceError> {
    count_linux_system_config_by_kind(conn, config_kind)
}

pub(super) fn query_linux_system_config_by_kind_paged(
    conn: &Connection,
    config_kind: &str,
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    return Ok(artifact_records(conn)?
        .into_iter()
        .filter(|record| {
            record.artifact_type == "LinuxSystemConfig"
                && record.attrs.get("configKind").and_then(Value::as_str) == Some(config_kind)
                && record.attrs.get("overlayContext").is_none()
        })
        .skip(offset as usize)
        .take(limit as usize)
        .map(artifact_row)
        .collect());
    #[allow(unreachable_code)]
    let sql = "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type = 'LinuxSystemConfig'
           AND json_extract(attrs, '$.configKind') = ?1
           AND json_extract(attrs, '$.overlayContext') IS NULL
         ORDER BY id ASC LIMIT ?2 OFFSET ?3";
    query_artifact_rows_with_statement(
        conn,
        sql,
        &[
            &config_kind as &dyn rusqlite::types::ToSql,
            &(limit as i64),
            &(offset as i64),
        ],
    )
}

/// Fetch passwdAccount and shadowAccount rows for the merged account list.
/// Unpaged on purpose: local account counts are in the low hundreds at most.
pub(super) fn query_linux_account_rows(
    conn: &Connection,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    return Ok(artifact_records(conn)?
        .into_iter()
        .filter(|record| {
            record.artifact_type == "LinuxSystemConfig"
                && matches!(
                    record.attrs.get("configKind").and_then(Value::as_str),
                    Some("passwdAccount" | "shadowAccount")
                )
                && record.attrs.get("overlayContext").is_none()
        })
        .map(artifact_row)
        .collect());
    #[allow(unreachable_code)]
    let sql = "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type = 'LinuxSystemConfig'
           AND json_extract(attrs, '$.configKind') IN ('passwdAccount', 'shadowAccount')
           AND json_extract(attrs, '$.overlayContext') IS NULL
         ORDER BY id ASC";
    query_artifact_rows_with_statement(conn, sql, &[])
}

/// Fetch `/etc/hostname` text-config rows ordered by source line number so the
/// first non-empty line wins.
pub(super) fn query_linux_hostname_rows(
    conn: &Connection,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, AnalysisServiceError> {
    return Ok(artifact_records(conn)?
        .into_iter()
        .filter(|record| {
            record.artifact_type == "LinuxSystemConfig"
                && record
                    .attrs
                    .get("sourcePath")
                    .and_then(Value::as_str)
                    .is_some_and(|path| {
                        path.ends_with("/etc/hostname") || path.ends_with("etc/hostname")
                    })
                && record.attrs.get("overlayContext").is_none()
        })
        .take(limit as usize)
        .map(artifact_row)
        .collect());
    #[allow(unreachable_code)]
    let sql = "SELECT id, source_object_id, extractor_id, created_at, attrs
         FROM artifacts
         WHERE artifact_type = 'LinuxSystemConfig'
           AND SUBSTR(json_extract(attrs, '$.sourcePath'), -13) IN ('/etc/hostname', 'etc/hostname')
           AND json_extract(attrs, '$.overlayContext') IS NULL
         ORDER BY CAST(json_extract(attrs, '$.lineNumber') AS INTEGER) ASC, id ASC
         LIMIT ?1";
    query_artifact_rows_with_statement(conn, sql, &[&(limit as i64)])
}

pub(super) fn status_from_total(total: u64) -> AnalysisParseStatusDto {
    if total > 0 {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotFound
    }
}

fn artifact_records(
    conn: &Connection,
) -> Result<Vec<persistence_fjall::ArtifactRecord>, AnalysisServiceError> {
    persistence_sqlite::repositories::artifact_repo::ArtifactRepo::new(conn)
        .list_all_records()
        .map_err(Into::into)
}

fn artifact_row(record: persistence_fjall::ArtifactRecord) -> AnalysisArtifactRow {
    let created_at = record
        .attrs
        .get("createdAt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let attrs = record
        .attrs
        .get("attrs")
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .unwrap_or_default();
    AnalysisArtifactRow {
        id: record.id,
        source_object_id: record.source_object_id,
        extractor_id: record
            .attrs
            .get("extractorId")
            .and_then(Value::as_str)
            .map(str::to_owned),
        created_at,
        attrs,
    }
}

fn analysis_families() -> Vec<&'static str> {
    vec![
        "RegistryValue",
        "RegistrySamUser",
        "RegistryUserAssist",
        "RegistryHive",
        "RegistryNetworkAdapter",
        "RegistryNetworkProfile",
        "RegistryInstalledSoftware",
        "RegistryPortableDevice",
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
        "BrowserHistory",
        "BrowserDownload",
        "BrowserCookie",
        "BrowserSessionTab",
        "BrowserPassword",
        "BrowserExtension",
        "EmailMessage",
        "EmailAccount",
        "EvtxBootShutdown",
        "EvtxSecurityEvent",
        "EvtxApplicationEvent",
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
    ]
}
