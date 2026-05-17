use domain::CaseMeta;
use persistence_sqlite::repositories::{file_repo::FileRepo, timeline_repo::TimelineRepo};
use reports::{CsvExporter, HtmlReportExporter, JsonExporter};
use rusqlite::Connection;
use std::fs;
use std::path::Path;
use transport::dto::{ReportHistoryItemDto, ReportTemplateDto};
use uuid::Uuid;

pub fn generate_html_report(
    conn: &Connection,
    case: &CaseMeta,
    output_dir: &Path,
) -> Result<String, String> {
    let file_repo = FileRepo::new(conn);
    let tl_repo = TimelineRepo::new(conn);

    let file_count = file_repo
        .count_by_data_source(&domain::DataSourceId("__all__".into()))
        .unwrap_or(0);

    let tl_count = tl_repo.count().map_err(|e| e.to_string())?;

    let files = vec![format!("{} files indexed", file_count)];
    let artifacts = vec![format!("{} timeline events", tl_count)];

    let file_name = format!("report-{}.html", Uuid::new_v4());
    let path = output_dir.join(&file_name);
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    HtmlReportExporter::export(&mut f, case, &files, &artifacts).map_err(|e| e.to_string())?;

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
    let json_val = serde_json::json!({
        "timeline_events": events.iter().map(|e| serde_json::json!({
            "id": e.id.0,
            "type": e.event_type,
            "ts": e.timestamp.to_rfc3339(),
            "title": e.title,
        })).collect::<Vec<_>>(),
    });

    let file_name = format!("export-{}.json", Uuid::new_v4());
    let path = output_dir.join(&file_name);
    let mut f = fs::File::create(&path).map_err(|e| e.to_string())?;
    JsonExporter::export(&mut f, &json_val).map_err(|e| e.to_string())?;

    Ok(file_name)
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
    vec![
        ReportHistoryItemDto {
            id: "report-job-001".into(),
            file_name: "case-summary-2025-02-16.pdf".into(),
            created_by: "取证分析员 A".into(),
            created_at: "2025-02-16T19:00:00Z".into(),
            status: "completed".into(),
            progress: None,
        },
        ReportHistoryItemDto {
            id: "report-job-002".into(),
            file_name: "file-activity-2025-02-16.pdf".into(),
            created_by: "取证分析员 A".into(),
            created_at: "2025-02-16T19:12:00Z".into(),
            status: "running".into(),
            progress: Some(64),
        },
    ]
}
