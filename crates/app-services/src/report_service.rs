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
