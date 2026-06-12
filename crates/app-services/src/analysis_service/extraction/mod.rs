pub(crate) mod browser;
pub(crate) mod email;
pub(crate) mod registry;

use self::browser::extract_browser_candidate;
use self::email::extract_email_candidate;
use self::registry::extract_registry_candidate;
use crate::analysis_service::candidates::{evidence_candidates_for_categories, EvidenceCandidate};
use crate::analysis_service::MAX_ANALYSIS_SOURCE_BYTES;
use chrono::Utc;
use domain::{Artifact, FileEntryId, TimelineEvent};
use persistence_sqlite::repositories::{artifact_repo::ArtifactRepo, timeline_repo::TimelineRepo};
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use transport::dto::{
    AnalysisExtractionRunDto, AnalysisParseStatusDto, BrowserDownloadDto, BrowserHistorySummaryDto,
    BrowserVisitDto, EmailExtractionSummaryDto, EmailMessageDto, RegistryExtractionSummaryDto,
    RegistryValueDto,
};

pub fn run_analysis_extraction(
    conn: &Connection,
    case_id: &str,
    categories: &[&str],
    mut file_reader: impl FnMut(&FileEntryId) -> Result<Box<dyn Read>, String>,
) -> Result<AnalysisExtractionRunDto, String> {
    let generated_at = Utc::now().to_rfc3339();
    let selected = if categories.is_empty() {
        vec!["Registry", "BrowserHistory", "Email"]
    } else {
        categories.to_vec()
    };
    let candidates = evidence_candidates_for_categories(conn, &selected)?;
    let mut artifacts = Vec::new();
    let mut events = Vec::new();
    let mut warnings = Vec::new();
    let mut scanned_count = 0u64;

    for candidate in candidates {
        if !matches!(
            candidate.category.as_str(),
            "Registry" | "BrowserHistory" | "Email"
        ) {
            continue;
        }
        if already_has_v1_artifacts(conn, &candidate)? {
            continue;
        }

        let mut reader = match file_reader(&candidate.file_id) {
            Ok(reader) => reader,
            Err(err) => {
                warnings.push(format!("{} read failed: {}", candidate.path, err));
                continue;
            }
        };
        let mut bytes = Vec::new();
        if let Err(err) = reader
            .by_ref()
            .take(MAX_ANALYSIS_SOURCE_BYTES as u64)
            .read_to_end(&mut bytes)
        {
            warnings.push(format!("{} read failed: {}", candidate.path, err));
            continue;
        }

        scanned_count += 1;
        let outcome = match candidate.category.as_str() {
            "Registry" => extract_registry_candidate(&candidate, &bytes),
            "BrowserHistory" => extract_browser_candidate(&candidate, &bytes),
            "Email" => extract_email_candidate(&candidate, &bytes),
            _ => ExtractionOutcome::default(),
        };
        warnings.extend(outcome.warnings);
        artifacts.extend(outcome.artifacts);
        events.extend(outcome.timeline_events);
    }

    if !artifacts.is_empty() {
        let by_source = artifacts_by_data_source(artifacts);
        let repo = ArtifactRepo::new(conn);
        for (data_source_id, group) in by_source {
            repo.insert_batch(&group, case_id, &data_source_id)
                .map_err(|e| e.to_string())?;
        }
    }
    if !events.is_empty() {
        TimelineRepo::new(conn)
            .insert_batch_with_case(&events, case_id)
            .map_err(|e| e.to_string())?;
    }

    let artifact_count = count_analysis_artifacts(conn)?;
    Ok(AnalysisExtractionRunDto {
        status: if scanned_count == 0 {
            AnalysisParseStatusDto::NotFound
        } else if warnings.is_empty() {
            AnalysisParseStatusDto::Parsed
        } else {
            AnalysisParseStatusDto::Partial
        },
        scanned_count,
        artifact_count,
        timeline_event_count: events.len() as u64,
        generated_at,
        warnings,
    })
}

pub fn get_registry_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<RegistryExtractionSummaryDto, String> {
    let total = count_artifacts_by_type(conn, "RegistryValue")?;
    let rows = query_artifact_rows(conn, &["RegistryValue"], offset, limit)?;
    let values = rows
        .into_iter()
        .map(|row| RegistryValueDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            hive_path: string_attr(&row.attrs, "hivePath"),
            key_path: string_attr(&row.attrs, "keyPath"),
            value_name: string_attr(&row.attrs, "valueName"),
            value_type: string_attr(&row.attrs, "valueType"),
            data: string_attr(&row.attrs, "data"),
            parser: row
                .extractor_id
                .unwrap_or_else(|| "registry.v1".to_string()),
            created_at: row.created_at,
        })
        .collect::<Vec<_>>();
    Ok(RegistryExtractionSummaryDto {
        status: status_from_total(total),
        total,
        values,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

pub fn get_browser_history_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<BrowserHistorySummaryDto, String> {
    let visit_total = count_artifacts_by_type(conn, "BrowserHistory")?;
    let download_total = count_artifacts_by_type(conn, "BrowserDownload")?;
    let visit_rows = query_artifact_rows(conn, &["BrowserHistory"], offset, limit)?;
    let download_rows = query_artifact_rows(conn, &["BrowserDownload"], offset, limit)?;
    let visits = visit_rows
        .into_iter()
        .map(|row| BrowserVisitDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            title: string_attr(&row.attrs, "title"),
            visit_time: optional_string_attr(&row.attrs, "visitTime"),
            visit_count: u64_attr(&row.attrs, "visitCount"),
        })
        .collect::<Vec<_>>();
    let downloads = download_rows
        .into_iter()
        .map(|row| BrowserDownloadDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            target_path: string_attr(&row.attrs, "targetPath"),
            start_time: optional_string_attr(&row.attrs, "startTime"),
            total_bytes: u64_attr(&row.attrs, "totalBytes"),
        })
        .collect::<Vec<_>>();
    Ok(BrowserHistorySummaryDto {
        status: status_from_total(visit_total + download_total),
        visit_total,
        download_total,
        visits,
        downloads,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

pub fn get_email_extraction_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<EmailExtractionSummaryDto, String> {
    let total = count_artifacts_by_type(conn, "EmailMessage")?;
    let rows = query_artifact_rows(conn, &["EmailMessage"], offset, limit)?;
    let messages = rows
        .into_iter()
        .map(|row| EmailMessageDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            sent_at: optional_string_attr(&row.attrs, "sentAt"),
            from: string_attr(&row.attrs, "from"),
            to: string_vec_attr(&row.attrs, "to"),
            cc: string_vec_attr(&row.attrs, "cc"),
            bcc: string_vec_attr(&row.attrs, "bcc"),
            subject: string_attr(&row.attrs, "subject"),
            message_id: string_attr(&row.attrs, "messageId"),
            attachments: string_vec_attr(&row.attrs, "attachments"),
            body_preview: string_attr(&row.attrs, "bodyPreview"),
        })
        .collect::<Vec<_>>();
    Ok(EmailExtractionSummaryDto {
        status: status_from_total(total),
        total,
        messages,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

#[derive(Default)]
struct ExtractionOutcome {
    artifacts: Vec<Artifact>,
    timeline_events: Vec<TimelineEvent>,
    warnings: Vec<String>,
}

struct AnalysisArtifactRow {
    id: String,
    source_object_id: Option<String>,
    extractor_id: Option<String>,
    created_at: String,
    attrs: BTreeMap<String, Value>,
}

fn already_has_v1_artifacts(
    conn: &Connection,
    candidate: &EvidenceCandidate,
) -> Result<bool, String> {
    let families = match candidate.category.as_str() {
        "Registry" => &["RegistryValue"][..],
        "BrowserHistory" => &["BrowserHistory", "BrowserDownload"][..],
        "Email" => &["EmailMessage"][..],
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
        "SELECT COUNT(*) FROM artifacts WHERE source_object_id = ?1 AND artifact_type IN ({})",
        placeholders
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
    let count: i64 = conn
        .query_row(&sql, params_refs.as_slice(), |row| row.get(0))
        .map_err(|e| e.to_string())?;
    Ok(count > 0)
}

fn artifacts_by_data_source(artifacts: Vec<Artifact>) -> HashMap<String, Vec<Artifact>> {
    let mut grouped: HashMap<String, Vec<Artifact>> = HashMap::new();
    for artifact in artifacts {
        let data_source_id = artifact
            .attrs
            .get("dataSourceId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        grouped.entry(data_source_id).or_default().push(artifact);
    }
    grouped
}

fn count_analysis_artifacts(conn: &Connection) -> Result<u64, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type IN ('RegistryValue', 'BrowserHistory', 'BrowserDownload', 'EmailMessage')",
            [],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count as u64)
}

fn count_artifacts_by_type(conn: &Connection, artifact_type: &str) -> Result<u64, String> {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM artifacts WHERE artifact_type = ?1",
            [artifact_type],
            |row| row.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(count as u64)
}

fn query_artifact_rows(
    conn: &Connection,
    families: &[&str],
    offset: u64,
    limit: u32,
) -> Result<Vec<AnalysisArtifactRow>, String> {
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
    let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
            let attrs_text: String = row.get(4)?;
            Ok(AnalysisArtifactRow {
                id: row.get(0)?,
                source_object_id: row.get(1)?,
                extractor_id: row.get(2)?,
                created_at: row.get(3)?,
                attrs: serde_json::from_str(&attrs_text).unwrap_or_default(),
            })
        })
        .map_err(|e| e.to_string())?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row.map_err(|e| e.to_string())?);
    }
    Ok(result)
}

fn status_from_total(total: u64) -> AnalysisParseStatusDto {
    if total > 0 {
        AnalysisParseStatusDto::Parsed
    } else {
        AnalysisParseStatusDto::NotFound
    }
}

fn string_attr(attrs: &BTreeMap<String, Value>, key: &str) -> String {
    attrs
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default()
}

fn optional_string_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    attrs
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn u64_attr(attrs: &BTreeMap<String, Value>, key: &str) -> u64 {
    attrs.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn string_vec_attr(attrs: &BTreeMap<String, Value>, key: &str) -> Vec<String> {
    attrs
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}
