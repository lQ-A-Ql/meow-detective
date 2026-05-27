use persistence_sqlite::repositories::job_repo::JobRepo;
use rusqlite::Connection;
use transport::dto::{JobSnapshotDto, TraceItemDto, WarningItemDto};

pub fn get_jobs_from_db(conn: &Connection) -> Result<Vec<JobSnapshotDto>, String> {
    let repo = JobRepo::new(conn);
    let jobs = repo.list_recent(12).map_err(|e| e.to_string())?;
    let dtos = jobs
        .into_iter()
        .map(|job| {
            let meta = parse_partition_progress(&job.detail);
            JobSnapshotDto {
                id: job.id.0,
                name: job.kind,
                scope: meta
                    .as_ref()
                    .and_then(|item| item.scope.clone())
                    .unwrap_or_else(|| {
                        if job.detail.is_empty() {
                            "Case ingest".to_string()
                        } else {
                            job.detail.clone()
                        }
                    }),
                progress: job.progress,
                status: job.status,
                detail: meta
                    .as_ref()
                    .map(|item| item.detail.clone())
                    .unwrap_or(job.detail),
                current_partition: meta
                    .as_ref()
                    .and_then(|item| item.current_partition.clone()),
                completed_partitions: meta.as_ref().map(|item| item.completed_partitions),
                total_partitions: meta.as_ref().map(|item| item.total_partitions),
                partition_progress: meta.as_ref().map(|item| item.partition_progress),
            }
        })
        .collect();
    Ok(dtos)
}

pub fn get_jobs_snapshot() -> Vec<JobSnapshotDto> {
    vec![
        JobSnapshotDto {
            id: "job-001".into(),
            name: "索引用户目录".into(),
            scope: "C:/Users/Alice".into(),
            progress: 72,
            status: "running".into(),
            detail: "已扫描 128,440 个对象".into(),
            current_partition: None,
            completed_partitions: None,
            total_partitions: None,
            partition_progress: None,
        },
        JobSnapshotDto {
            id: "job-002".into(),
            name: "生成案件摘要".into(),
            scope: "报告导出".into(),
            progress: 100,
            status: "completed".into(),
            detail: "PDF 已写入导出目录".into(),
            current_partition: None,
            completed_partitions: None,
            total_partitions: None,
            partition_progress: None,
        },
        JobSnapshotDto {
            id: "job-003".into(),
            name: "提取浏览器历史".into(),
            scope: "Chrome".into(),
            progress: 100,
            status: "warning".into(),
            detail: "部分 SQLite 页面已损坏，已跳过。".into(),
            current_partition: None,
            completed_partitions: None,
            total_partitions: None,
            partition_progress: None,
        },
    ]
}

pub fn get_warnings() -> Vec<WarningItemDto> {
    vec![
        WarningItemDto {
            id: "warn-001".into(),
            title: "浏览器历史部分损坏".into(),
            detail: "Chrome History 数据库存在损坏页，提取结果可能不完整。".into(),
        },
        WarningItemDto {
            id: "warn-002".into(),
            title: "发现可疑远控工具".into(),
            detail: "下载目录中存在 AnyDesk.exe，建议结合时间线进一步核查。".into(),
        },
    ]
}

pub fn get_trace_items() -> Vec<TraceItemDto> {
    vec![
        TraceItemDto {
            id: "trace-001".into(),
            ts: "2025-02-16T19:10:00Z".into(),
            message: "job.index.users progress=72 scanned=128440".into(),
        },
        TraceItemDto {
            id: "trace-002".into(),
            ts: "2025-02-16T19:12:04Z".into(),
            message: "report.summary export started template=report-summary".into(),
        },
    ]
}

#[derive(Debug, Clone)]
struct PartitionProgressMeta {
    scope: Option<String>,
    detail: String,
    current_partition: Option<String>,
    completed_partitions: u32,
    total_partitions: u32,
    partition_progress: u32,
}

fn parse_partition_progress(detail: &str) -> Option<PartitionProgressMeta> {
    let payload = detail.strip_prefix("[partition-progress] ")?;
    let mut parts = payload.splitn(5, '|');
    let completed: u32 = parts.next()?.parse().ok()?;
    let total: u32 = parts.next()?.parse().ok()?;
    let partition_progress: u32 = parts.next()?.parse().ok()?;
    let current_partition = parts.next()?.to_string();
    let human_detail = parts.next()?.to_string();

    Some(PartitionProgressMeta {
        scope: Some(format!(
            "分区 {}/{}",
            completed.saturating_add(1).min(total.max(1)),
            total
        )),
        detail: human_detail,
        current_partition: Some(current_partition),
        completed_partitions: completed,
        total_partitions: total,
        partition_progress,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_partition_progress;

    #[test]
    fn parses_partition_progress_payload() {
        let meta = parse_partition_progress(
            "[partition-progress] 1|5|42|Partition 3 (NTFS) - Basic data partition|Enumerating Partition 3 (NTFS) - Basic data partition",
        )
        .expect("expected metadata");

        assert_eq!(meta.completed_partitions, 1);
        assert_eq!(meta.total_partitions, 5);
        assert_eq!(meta.partition_progress, 42);
        assert_eq!(
            meta.current_partition.as_deref(),
            Some("Partition 3 (NTFS) - Basic data partition")
        );
        assert_eq!(meta.scope.as_deref(), Some("分区 2/5"));
        assert_eq!(
            meta.detail,
            "Enumerating Partition 3 (NTFS) - Basic data partition"
        );
    }
}
