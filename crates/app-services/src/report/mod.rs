mod document;
mod error;
mod export;
mod history;
mod saving;

use std::{
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use transport::commands::ExportReportRequest;

pub use error::ReportError;
pub use export::export_report_for_case;
pub use history::get_report_history;
pub use saving::save_report_for_case;

pub struct ReportExportContext<'a> {
    pub connection: &'a rusqlite::Connection,
    pub case: &'a domain::CaseMeta,
    pub case_root: &'a Path,
    pub request: &'a ExportReportRequest,
    pub cancel: &'a AtomicBool,
}

fn ensure_active(cancel: &AtomicBool) -> Result<(), ReportError> {
    if cancel.load(Ordering::Acquire) {
        return Err(ReportError::Cancelled);
    }
    Ok(())
}

pub fn read_report_document_for_case(
    context: &ReportExportContext<'_>,
) -> Result<transport::dto::ReportDocumentDto, ReportError> {
    if context.request.case_id != context.case.id.0 {
        return Err(ReportError::NotFound(
            "report case does not match its context".into(),
        ));
    }
    document::load(context)
}

#[cfg(test)]
#[path = "../../tests/unit/report/mod.rs"]
mod tests;
