use app_services::parallel_enum::enumerate_ntfs_reader_to_staging;
use persistence_sqlite::{DbError, DbResult};

pub fn enumerate(
    source: &rusqlite::Connection,
    id: &domain::DataSourceId,
    image: &std::path::Path,
    offset: u64,
    partition: usize,
) -> DbResult<app_services::file_service::EnumerationStats> {
    let staging = rusqlite::Connection::open_in_memory()?;
    let reader = image_e01::E01Reader::open(image)?;
    let stats = enumerate_ntfs_reader_to_staging(
        &staging,
        Box::new(reader),
        &id.0,
        partition,
        offset,
        &std::sync::atomic::AtomicBool::new(false),
    )
    .map_err(|error| DbError::System(error.to_string()))?;
    persistence_source::repositories::staging_repo::StagingRepo::merge_enum_staging_to_main(
        source,
        &staging,
        &id.0,
        partition,
        &format!("Partition {partition} (NTFS)"),
    )?;
    Ok(app_services::file_service::EnumerationStats {
        file_count: stats.file_count,
        dir_count: stats.dir_count,
        total_size: stats.total_size,
        warnings: (stats.directory_index_failures > 0)
            .then(|| {
                format!(
                    "NTFS directory index failures: {}",
                    stats.directory_index_failures
                )
            })
            .into_iter()
            .collect(),
        diagnostics: Vec::new(),
    })
}
