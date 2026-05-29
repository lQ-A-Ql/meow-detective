use persistence_sqlite::repositories::job_repo::JobRepo;
use rusqlite::Connection;
use transport::dto::JobSnapshotDto;

pub fn get_jobs_from_db(conn: &Connection) -> Result<Vec<JobSnapshotDto>, String> {
    let repo = JobRepo::new(conn);
    let jobs = repo
        .list_recent(infrastructure::constants::JOB_LIST_LIMIT)
        .map_err(|e| e.to_string())?;
    let dtos = jobs
        .into_iter()
        .map(|job| {
            // Use DB columns for partition progress if available, fall back to parsing detail
            let has_db_partition = job.total_partitions > 0;
            let meta = if has_db_partition {
                None // DB columns are authoritative
            } else {
                parse_partition_progress(&job.detail)
            };

            let current_partition = if has_db_partition {
                job.current_partition.clone()
            } else {
                meta.as_ref()
                    .and_then(|item| item.current_partition.clone())
            };
            let completed_partitions = if has_db_partition {
                Some(job.completed_partitions)
            } else {
                meta.as_ref().map(|item| item.completed_partitions)
            };
            let total_partitions = if has_db_partition {
                Some(job.total_partitions)
            } else {
                meta.as_ref().map(|item| item.total_partitions)
            };
            let partition_progress = if has_db_partition {
                Some(job.partition_progress)
            } else {
                meta.as_ref().map(|item| item.partition_progress)
            };

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
                current_partition,
                completed_partitions,
                total_partitions,
                partition_progress,
            }
        })
        .collect();
    Ok(dtos)
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
