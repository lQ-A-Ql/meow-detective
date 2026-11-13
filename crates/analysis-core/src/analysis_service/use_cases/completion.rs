use super::{extraction, runtime::AnalysisSourceReadRuntime};
use crate::analysis_service::{
    extraction::{AnalysisExtractionExecution, ExtractionProgressUpdate},
    AnalysisServiceError,
};
use domain::{CaseId, DataSourceId};
use evidence_access::file_service::SourceReadContext;
use rusqlite::Connection;
use std::{
    path::Path,
    time::{Duration, Instant},
};
use transport::dto::{AnalysisExtractionPhaseDto, AnalysisWorkProgressDto, AnalysisWorkStageDto};

pub(super) struct CompletionContext<'a> {
    pub case_conn: &'a Connection,
    pub case_root: &'a Path,
    pub case_id: &'a CaseId,
    pub source_id: &'a DataSourceId,
    pub runtime: &'a AnalysisSourceReadRuntime,
    pub admission_ms: u64,
    pub source_read_count: u64,
    pub source_read_elapsed: Duration,
}

pub(super) fn finish(
    context: CompletionContext<'_>,
    extraction: &mut Result<AnalysisExtractionExecution, AnalysisServiceError>,
    reader: &mut SourceReadContext<'_>,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
    terminal: Vec<ExtractionProgressUpdate>,
) -> Result<(), AnalysisServiceError> {
    if extraction.is_err() {
        for update in terminal
            .into_iter()
            .filter(|update| update.phase == AnalysisExtractionPhaseDto::Failed)
        {
            progress(update);
        }
        return Ok(());
    }
    let post_started = Instant::now();
    if let Ok(execution) = extraction {
        let audit = Instant::now();
        extraction::record_plugin_audit_trail(
            context.case_conn,
            context.case_id,
            context.source_id,
            &execution.plugin_loads,
            &execution.plugin_rejections,
            &execution.plugin_extract_failures,
        );
        let records = execution.plugin_loads.len()
            + execution.plugin_rejections.len()
            + execution.plugin_extract_failures.len();
        record_work(
            execution,
            progress,
            terminal.first(),
            AnalysisWorkStageDto::Audit,
            audit,
            records as u64,
        );
    }
    let locators = Instant::now();
    extraction::finish_extraction(extraction, context.source_id, context.runtime, reader)?;
    if let Ok(execution) = extraction {
        record_work(
            execution,
            progress,
            terminal.first(),
            AnalysisWorkStageDto::LocatorFlush,
            locators,
            0,
        );
        let completion = Instant::now();
        super::generation_gc::request(context.case_root, context.source_id)?;
        extraction::record_extraction_metrics(
            execution,
            context.admission_ms,
            context.source_read_count,
            context.source_read_elapsed,
            reader,
        );
        record_work(
            execution,
            progress,
            terminal.first(),
            AnalysisWorkStageDto::Completion,
            completion,
            0,
        );
    }
    for update in terminal {
        progress(update);
    }
    if let Ok(execution) = extraction {
        execution.persistence_elapsed_ms = execution
            .persistence_elapsed_ms
            .saturating_add(u64::try_from(post_started.elapsed().as_millis()).unwrap_or(u64::MAX));
    }
    Ok(())
}

fn record_work(
    execution: &mut AnalysisExtractionExecution,
    progress: &mut dyn FnMut(ExtractionProgressUpdate),
    terminal: Option<&ExtractionProgressUpdate>,
    stage: AnalysisWorkStageDto,
    started: Instant,
    records: u64,
) {
    let work = AnalysisWorkProgressDto {
        stage,
        elapsed_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        records,
        bytes: 0,
        complete: true,
    };
    execution.work_metrics.push(work.clone());
    if let Some(template) = terminal {
        let mut update = template.clone();
        update.phase = AnalysisExtractionPhaseDto::Persisting;
        update.detail = crate::analysis_service::extraction::PERSISTING_DETAIL.to_owned();
        update.work = Some(work);
        progress(update);
    }
}
