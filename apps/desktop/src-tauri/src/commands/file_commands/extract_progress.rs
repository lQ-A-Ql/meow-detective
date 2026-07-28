use app_services::file_service::{FileExtractionProgressPhase, FileExtractionProgressUpdate};
use tauri::AppHandle;
use transport::{
    dto::{FileExtractionPhaseDto, FileExtractionProgressDto, FileExtractionResultDto},
    CommandError,
};

use crate::events::event_bridge;

pub(super) fn emit_preparing(app: &AppHandle, operation_id: &str, file_id: &str) {
    emit(
        app,
        operation_id,
        file_id,
        FileExtractionPhaseDto::Preparing,
        0,
        None,
    );
}

pub(super) fn emit_copy_update(
    app: &AppHandle,
    operation_id: &str,
    file_id: &str,
    update: FileExtractionProgressUpdate,
) {
    let phase = match update.phase {
        FileExtractionProgressPhase::Copying => FileExtractionPhaseDto::Copying,
        FileExtractionProgressPhase::Finalizing => FileExtractionPhaseDto::Finalizing,
    };
    emit(
        app,
        operation_id,
        file_id,
        phase,
        update.bytes_written,
        update.total_bytes,
    );
}

pub(super) fn emit_terminal(
    app: &AppHandle,
    operation_id: &str,
    file_id: &str,
    result: &Result<FileExtractionResultDto, CommandError>,
    last_bytes_written: u64,
    last_total_bytes: Option<u64>,
) {
    match result {
        Ok(extraction) => {
            let phase = if extraction.audit_persisted {
                FileExtractionPhaseDto::Completed
            } else {
                FileExtractionPhaseDto::CompletedWithWarning
            };
            emit(
                app,
                operation_id,
                file_id,
                phase,
                extraction.bytes_written,
                extraction.source_size,
            );
        }
        Err(_) => emit(
            app,
            operation_id,
            file_id,
            FileExtractionPhaseDto::Failed,
            last_bytes_written,
            last_total_bytes,
        ),
    }
}

fn emit(
    app: &AppHandle,
    operation_id: &str,
    file_id: &str,
    phase: FileExtractionPhaseDto,
    bytes_written: u64,
    total_bytes: Option<u64>,
) {
    let percent = total_bytes.map(|total| {
        if total == 0 {
            100
        } else {
            bytes_written
                .saturating_mul(100)
                .saturating_div(total)
                .min(100) as u32
        }
    });
    event_bridge::emit_file_extraction_progress(
        app,
        &FileExtractionProgressDto {
            operation_id: operation_id.to_string(),
            file_id: file_id.to_string(),
            phase,
            bytes_written,
            total_bytes,
            percent,
        },
    );
}
