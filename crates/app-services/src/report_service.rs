use domain::CaseMeta;
use persistence_sqlite::repositories::{file_repo::FileRepo, timeline_repo::TimelineRepo};
use reports::{CsvExporter, HtmlReportExporter, JsonExporter};
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use transport::dto::{
    AnalysisFileClassificationDto, AnalysisProvenanceDto, AnalysisSystemInfoDto,
    ReportHistoryItemDto, ReportTemplateDto,
};
use uuid::Uuid;

pub fn generate_html_report(
    conn: &Connection,
    case: &CaseMeta,
    output_dir: &Path,
) -> Result<String, String> {
    let file_repo = FileRepo::new(conn);
    let tl_repo = TimelineRepo::new(conn);

    let file_count = file_repo.count_all().unwrap_or(0);

    let tl_count = tl_repo.count().map_err(|e| e.to_string())?;

    let files = vec![format!("{} files indexed", file_count)];
    let artifacts = vec![format!("{} timeline events", tl_count)];
    let analysis = current_analysis(conn)?;

    let file_name = format!("report-{}.html", Uuid::new_v4());
    let path = output_dir.join(&file_name);
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    HtmlReportExporter::export_with_analysis(
        &mut f,
        case,
        &files,
        &artifacts,
        &analysis_rows(&analysis.system_info, &analysis.classifications),
    )
    .map_err(|e| e.to_string())?;

    Ok(file_name)
}

pub fn generate_csv_artifacts(conn: &Connection, output_dir: &Path) -> Result<String, String> {
    let mut stmt = conn.prepare(
        "SELECT artifact_type, title, summary FROM artifacts ORDER BY created_at DESC LIMIT 1000"
    ).map_err(|e| e.to_string())?;
    let rows_data: Vec<Vec<String>> = stmt
        .query_map([], |row| {
            Ok(vec![
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ])
        })
        .map_err(|e| e.to_string())?
        .filter_map(|r| r.ok())
        .collect();
    let mut rows_data = rows_data;
    let analysis = current_analysis(conn)?;
    rows_data.extend(
        analysis_rows(&analysis.system_info, &analysis.classifications)
            .into_iter()
            .map(|row| vec!["analysis".to_string(), "provenance".to_string(), row]),
    );

    let file_name = format!("artifacts-{}.csv", Uuid::new_v4());
    let path = output_dir.join(&file_name);
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    CsvExporter::export_artifacts(&mut f, &["type", "title", "summary"], &rows_data)
        .map_err(|e| e.to_string())?;

    Ok(file_name)
}

pub fn generate_json_export(conn: &Connection, output_dir: &Path) -> Result<String, String> {
    let events = TimelineRepo::new(conn)
        .query(0, 500)
        .map_err(|e| e.to_string())?;
    let analysis = current_analysis(conn)?;
    let json_val = serde_json::json!({
        "timeline_events": events.iter().map(|e| serde_json::json!({
            "id": e.id.0,
            "type": e.event_type,
            "ts": e.timestamp.to_rfc3339(),
            "title": e.title,
        })).collect::<Vec<_>>(),
        "analysis": {
            "systemInfo": analysis.system_info,
            "classifications": analysis.classifications,
            "summary": crate::analysis_service::generate_analysis_summary(
                &analysis.system_info,
                &analysis.classifications,
            ),
        },
    });

    let file_name = format!("export-{}.json", Uuid::new_v4());
    let path = output_dir.join(&file_name);
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    JsonExporter::export(&mut f, &json_val).map_err(|e| e.to_string())?;

    Ok(file_name)
}

struct ReportAnalysis {
    system_info: AnalysisSystemInfoDto,
    classifications: Vec<AnalysisFileClassificationDto>,
}

fn current_analysis(conn: &Connection) -> Result<ReportAnalysis, String> {
    let system_info =
        crate::analysis_service::extract_system_info_for_case(conn, |file_id, max_bytes| {
            crate::file_service::read_file_header_by_id(conn, file_id, max_bytes)
        });
    let files = crate::analysis_service::collect_file_entries(conn)?;
    let classifications = crate::analysis_service::classify_files_by_magic(
        &files,
        crate::analysis_service::DEFAULT_SAMPLE_SIZE,
        |file_id| {
            crate::file_service::read_file_header_by_id(
                conn,
                file_id,
                crate::analysis_service::MAGIC_HEADER_LIMIT,
            )
        },
    );

    Ok(ReportAnalysis {
        system_info,
        classifications,
    })
}

fn analysis_rows(
    system_info: &AnalysisSystemInfoDto,
    classifications: &[AnalysisFileClassificationDto],
) -> Vec<String> {
    let mut rows = Vec::new();
    rows.push(format!(
        "system_info status={} warnings={}",
        status_str(&system_info.status),
        system_info.warnings.join(" | ")
    ));
    rows.extend(
        system_info
            .provenance
            .iter()
            .map(|item| format_provenance("system_info", item)),
    );

    for classification in classifications {
        rows.push(format!(
            "classification category={} status={} warnings={}",
            classification.category,
            status_str(&classification.status),
            classification.warnings.join(" | ")
        ));
        rows.extend(
            classification
                .provenance
                .iter()
                .map(|item| format_provenance(&classification.category, item)),
        );
    }

    rows
}

fn format_provenance(scope: &str, item: &AnalysisProvenanceDto) -> String {
    format!(
        "{} parser={} status={} dataSource={} artifact={} parsedAt={} warnings={}",
        scope,
        item.parser,
        status_str(&item.status),
        item.data_source_id,
        item.artifact_path,
        item.parsed_at,
        item.warnings.join(" | ")
    )
}

fn status_str(status: &transport::dto::AnalysisParseStatusDto) -> &'static str {
    match status {
        transport::dto::AnalysisParseStatusDto::Parsed => "parsed",
        transport::dto::AnalysisParseStatusDto::NotParsed => "notParsed",
        transport::dto::AnalysisParseStatusDto::Unavailable => "unavailable",
    }
}

pub fn get_report_templates() -> Vec<ReportTemplateDto> {
    vec![
        ReportTemplateDto {
            id: "report-summary".into(),
            name: "案件摘要报告".into(),
            description: "输出案件基础信息、关键时间线与工件摘要。".into(),
        },
        ReportTemplateDto {
            id: "report-files".into(),
            name: "文件活动报告".into(),
            description: "输出可疑文件、哈希与访问活动。".into(),
        },
    ]
}

pub fn get_report_history() -> Vec<ReportHistoryItemDto> {
    // TODO: implement real report history from database
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use domain::{
        CaseId, CaseMeta, DataSource, DataSourceId, DataSourceKind, EntryType, FileEntry,
        FileEntryId,
    };
    use persistence_sqlite::repositories::{
        case_repo::CaseRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
    };
    use persistence_sqlite::{open_in_memory, runner};
    use tempfile::TempDir;

    fn setup_report_case() -> (rusqlite::Connection, TempDir, CaseMeta, DataSourceId) {
        let conn = open_in_memory().unwrap();
        runner::run_all(&conn).unwrap();
        let case = CaseMeta {
            id: CaseId("case-report".to_string()),
            name: "<Report Case>".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        CaseRepo::new(&conn).create(&case).unwrap();

        let tmp = TempDir::new().unwrap();
        let ds_id = DataSourceId("ds-report".to_string());
        DataSourceRepo::new(&conn)
            .insert(
                &case.id,
                &DataSource {
                    id: ds_id.clone(),
                    name: "logical".to_string(),
                    kind: DataSourceKind::LogicalDirectory,
                    source_path: tmp.path().to_path_buf(),
                    imported_at: chrono::Utc::now(),
                },
            )
            .unwrap();

        (conn, tmp, case, ds_id)
    }

    fn insert_file(conn: &rusqlite::Connection, ds_id: &DataSourceId, id: &str, path: &str) {
        FileRepo::new(conn)
            .insert_batch(&[FileEntry {
                id: FileEntryId(id.to_string()),
                parent_id: None,
                data_source_id: ds_id.clone(),
                path: path.to_string(),
                name: path.rsplit(['/', '\\']).next().unwrap_or(path).to_string(),
                entry_type: EntryType::File,
                size: Some(4),
                ext: None,
                deleted: false,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            }])
            .unwrap();
    }

    #[test]
    fn json_export_includes_analysis_provenance_without_fake_facts() {
        let (conn, tmp, _case, ds_id) = setup_report_case();
        insert_file(&conn, &ds_id, "system", "Windows/System32/config/SYSTEM");

        let file_name = generate_json_export(&conn, tmp.path()).unwrap();
        let json = std::fs::read_to_string(tmp.path().join(file_name)).unwrap();

        assert!(json.contains("\"analysis\""));
        assert!(json.contains("\"provenance\""));
        assert!(json.contains("registry.system"));
        assert!(!json.contains("FORENSICS-PC"));
        assert!(!json.contains("Windows 10"));
    }

    #[test]
    fn html_report_escapes_analysis_provenance() {
        let (conn, tmp, case, ds_id) = setup_report_case();
        insert_file(
            &conn,
            &ds_id,
            "evil",
            "Windows/System32/config/<script>alert(1)</script>",
        );

        let file_name = generate_html_report(&conn, &case, tmp.path()).unwrap();
        let html = std::fs::read_to_string(tmp.path().join(file_name)).unwrap();

        assert!(html.contains("Analysis Provenance"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn csv_report_keeps_formula_sanitization_for_analysis_rows() {
        let (conn, tmp, _case, ds_id) = setup_report_case();
        insert_file(&conn, &ds_id, "formula", "=SUM(A1:A2)");
        conn.execute(
            "INSERT INTO artifacts (id, case_id, data_source_id, artifact_type, title, summary, attrs, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            rusqlite::params![
                "artifact-formula",
                "case-report",
                ds_id.0,
                "lnk",
                "=SUM(A1:A2)",
                "formula title fixture",
                "{}",
                chrono::Utc::now().to_rfc3339(),
            ],
        )
        .unwrap();

        let file_name = generate_csv_artifacts(&conn, tmp.path()).unwrap();
        let csv = std::fs::read_to_string(tmp.path().join(file_name)).unwrap();

        assert!(csv.contains("\"analysis\""));
        assert!(csv.contains("provenance"));
        assert!(csv.contains("\"\t=SUM(A1:A2)\""));
    }
}
