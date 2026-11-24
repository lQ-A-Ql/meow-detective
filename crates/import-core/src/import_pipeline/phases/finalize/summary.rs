pub(super) fn build(
    source_name: &str,
    stats: &evidence_access::file_service::EnumerationStats,
    pipeline_message: &str,
) -> String {
    let mut message = format!(
        "Imported {source_name}: {} files, {} dirs",
        stats.file_count, stats.dir_count
    );
    if !pipeline_message.is_empty() {
        message.push_str(". ");
        message.push_str(pipeline_message);
    }
    message
}
