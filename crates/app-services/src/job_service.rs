use persistence_sqlite::repositories::job_repo::JobRepo;
use rusqlite::Connection;
use transport::dto::{JobSnapshotDto, TraceItemDto, WarningItemDto};

pub fn get_jobs_from_db(conn: &Connection) -> Result<Vec<JobSnapshotDto>, String> {
    let repo = JobRepo::new(conn);
    let jobs = repo.list_active().map_err(|e| e.to_string())?;
    let dtos = jobs.into_iter().map(|(id, kind, status, progress)| {
        JobSnapshotDto {
            id: id.0,
            name: kind,
            scope: String::new(),
            progress,
            status,
            detail: String::new(),
        }
    }).collect();
    Ok(dtos)
}

pub fn get_jobs_snapshot() -> Vec<JobSnapshotDto> {
    vec![
        JobSnapshotDto { id: "job-001".into(), name: "索引用户目录".into(), scope: "C:/Users/Alice".into(), progress: 72, status: "running".into(), detail: "已扫描 128,440 个对象".into() },
        JobSnapshotDto { id: "job-002".into(), name: "生成案件摘要".into(), scope: "报告导出".into(), progress: 100, status: "completed".into(), detail: "PDF 已写入导出目录".into() },
        JobSnapshotDto { id: "job-003".into(), name: "提取浏览器历史".into(), scope: "Chrome".into(), progress: 100, status: "warning".into(), detail: "部分 SQLite 页面已损坏，已跳过。".into() },
    ]
}

pub fn get_warnings() -> Vec<WarningItemDto> {
    vec![
        WarningItemDto { id: "warn-001".into(), title: "浏览器历史部分损坏".into(), detail: "Chrome History 数据库存在损坏页，提取结果可能不完整。".into() },
        WarningItemDto { id: "warn-002".into(), title: "发现可疑远控工具".into(), detail: "下载目录中存在 AnyDesk.exe，建议结合时间线进一步核查。".into() },
    ]
}

pub fn get_trace_items() -> Vec<TraceItemDto> {
    vec![
        TraceItemDto { id: "trace-001".into(), ts: "2025-02-16T19:10:00Z".into(), message: "job.index.users progress=72 scanned=128440".into() },
        TraceItemDto { id: "trace-002".into(), ts: "2025-02-16T19:12:04Z".into(), message: "report.summary export started template=report-summary".into() },
    ]
}
