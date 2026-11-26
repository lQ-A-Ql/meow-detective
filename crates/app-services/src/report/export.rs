use super::{ReportError, ReportExportContext};
use persistence_sqlite::repositories::report_repo::{ReportRecord, ReportRepo};
use sha2::{Digest, Sha256};
use std::io::{Read, Seek, SeekFrom, Write};
use transport::dto::ReportHistoryItemDto;

pub fn export_report_for_case(
    context: ReportExportContext<'_>,
) -> Result<ReportHistoryItemDto, ReportError> {
    let id = uuid::Uuid::new_v4().to_string();
    let record = ReportRecord {
        id: id.clone(),
        case_id: context.case.id.0.clone(),
        template_id: "structured-summary".into(),
        file_name: format!("report-{id}.{}", context.request.format.extension()),
        created_by: context.case.examiner.clone().unwrap_or_default(),
        status: "running".into(),
        progress: Some(0),
        created_at: chrono::Utc::now().to_rfc3339(),
        request_json: serde_json::to_string(context.request)
            .map_err(|error| ReportError::Other(error.to_string()))?,
        error_code: None,
        error_message: None,
        file_size: None,
        sha256: None,
    };
    let repo = ReportRepo::new(context.connection);
    repo.insert(&record)?;
    let generated = generate(&context, &record);
    match generated {
        Ok(()) => {}
        Err(error) => {
            let (status, code, message) = failure_details(&error);
            tracing::error!(report_id = %id, error = %error, "Report export failed");
            repo.fail(&id, status, code, message)?;
            return Err(error);
        }
    }
    let record = repo
        .find_by_id(&context.case.id.0, &id)?
        .ok_or_else(|| ReportError::NotFound("report history record is missing".into()))?;
    super::history::to_dto(record)
}

fn failure_details(error: &ReportError) -> (&'static str, &'static str, &'static str) {
    use transport::{ErrorCategory, ServiceErrorCategory};
    match error.category() {
        ErrorCategory::Cancelled => ("cancelled", "REPORT_CANCELLED", "报告生成已取消"),
        ErrorCategory::Io => ("failed", "REPORT_IO", "报告文件读取或写入失败"),
        ErrorCategory::Validation => (
            "failed",
            "REPORT_SOURCE_UNAVAILABLE",
            "报告来源尚未准备完成",
        ),
        ErrorCategory::Unsupported => ("failed", "REPORT_UNSUPPORTED", "报告来源暂不支持所选分析"),
        ErrorCategory::Security => ("failed", "REPORT_DESTINATION_BLOCKED", "报告保存位置不可用"),
        ErrorCategory::Timeout => ("failed", "REPORT_TIMEOUT", "报告生成超过执行时间"),
        _ => ("failed", "REPORT_DATA_INVALID", "报告摘要读取或生成失败"),
    }
}

fn generate(context: &ReportExportContext<'_>, record: &ReportRecord) -> Result<(), ReportError> {
    super::ensure_active(context.cancel)?;
    let document = super::read_report_document_for_case(context)?;
    ReportRepo::new(context.connection).update_status(&record.id, "running", Some(50))?;
    let destination = evidence_access::file_service::prepare_case_managed_export_destination(
        context.connection,
        context.case_root,
        &context.case.id,
        &context.case_root.join("reports").join(&record.file_name),
    )?;
    let output_dir = destination
        .parent()
        .ok_or_else(|| ReportError::Other("report destination has no parent".into()))?;
    let mut temporary = tempfile::NamedTempFile::new_in(output_dir)?;
    reports::SummaryReportExporter::export(
        temporary.as_file_mut(),
        &document,
        context.request.format,
    )
    .map_err(ReportError::Io)?;
    super::ensure_active(context.cancel)?;
    temporary.as_file_mut().flush()?;
    temporary.as_file().sync_all()?;
    temporary.as_file_mut().seek(SeekFrom::Start(0))?;
    let (size, hash) = hash_reader(temporary.as_file_mut())?;
    ReportRepo::new(context.connection).update_status(&record.id, "running", Some(90))?;
    super::ensure_active(context.cancel)?;
    let published = temporary
        .persist_noclobber(&destination)
        .map_err(|error| ReportError::Io(error.error))?;
    if let Err(error) = ReportRepo::new(context.connection).complete(&record.id, size, &hash) {
        drop(published);
        std::fs::remove_file(destination)?;
        return Err(error.into());
    }
    Ok(())
}

pub(super) fn hash_reader(reader: &mut impl Read) -> Result<(u64, String), ReportError> {
    let mut hasher = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let length = reader.read(&mut buffer)?;
        if length == 0 {
            break;
        }
        hasher.update(&buffer[..length]);
        bytes += length as u64;
    }
    Ok((bytes, hex::encode(hasher.finalize())))
}
