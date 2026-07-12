use persistence_sqlite::repositories::report_repo::ReportRepo;
use rusqlite::Connection;
use transport::dto::{ReportHistoryItemDto, ReportTemplateDto};

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

pub fn get_report_history(conn: &Connection, case_id: &str) -> Vec<ReportHistoryItemDto> {
    let records = match ReportRepo::new(conn).list_by_case(case_id) {
        Ok(records) => records,
        Err(_) => return Vec::new(),
    };
    records
        .into_iter()
        .map(|record| ReportHistoryItemDto {
            id: record.id,
            file_name: record.file_name,
            created_by: record.created_by,
            created_at: record.created_at,
            status: record.status,
            progress: record.progress,
        })
        .collect()
}
