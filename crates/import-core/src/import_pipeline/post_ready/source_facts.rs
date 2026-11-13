use crate::import_pipeline::context::ImportJobContext;
use crate::import_pipeline::profile::{emit_phase_profile, Counters};
use std::time::{Duration, Instant};
use transport::dto::import_benchmark::{
    ImportBenchmarkPhaseDto as Phase, ImportBenchmarkStateDto as State,
};

pub(super) fn cache_linux_facts(
    ctx: &ImportJobContext<'_>,
    source: &domain::DataSource,
    connection: &rusqlite::Connection,
) -> usize {
    if ctx.import_config.platform != domain::DataSourcePlatform::Linux {
        return 0;
    }
    let total_started = Instant::now();
    emit_started(ctx, "collect");
    let collect_started = Instant::now();
    let facts = analysis_core::linux_evidence_facts::collect_linux_evidence_facts(
        ctx.conn,
        ctx.case_root,
        ctx.case_id,
        &source.id,
    );
    record(
        ctx,
        Phase::SourceFactsCollect,
        "collect",
        collect_started.elapsed(),
        true,
        1,
    );

    emit_started(ctx, "persist");
    let persist_started = Instant::now();
    let persisted = analysis_core::linux_evidence_facts::persist_linux_evidence_facts(
        connection, &source.id, &facts,
    );
    let complete = persisted.is_ok();
    record(
        ctx,
        Phase::SourceFactsPersist,
        "persist",
        persist_started.elapsed(),
        complete,
        u64::from(complete),
    );
    record(
        ctx,
        Phase::SourceFacts,
        "total",
        total_started.elapsed(),
        complete,
        u64::from(complete),
    );
    if let Err(error) = persisted {
        tracing::warn!(data_source_id = %source.id.0, %error, "Linux evidence facts cache could not be persisted");
        return 1;
    }
    0
}

fn emit_started(ctx: &ImportJobContext<'_>, stage: &str) {
    emit_phase_profile(
        ctx.event_sink(),
        ctx.job_id,
        ctx.case_id,
        ctx.ds.map(|source| &source.id),
        96,
        format!("Source facts: phase=source-facts stage={stage} state=running"),
        ctx.cancel_requested(),
    );
}

fn record(
    ctx: &ImportJobContext<'_>,
    phase: Phase,
    stage: &str,
    elapsed: Duration,
    complete: bool,
    objects: u64,
) {
    let state = if complete {
        State::Completed
    } else {
        State::Failed
    };
    if let Some(profiler) = &ctx.profiler {
        profiler.record(
            None,
            phase,
            Some(elapsed),
            state,
            Counters {
                staged: objects,
                ..Default::default()
            },
        );
    }
    let state = if complete { "completed" } else { "failed" };
    emit_phase_profile(
        ctx.event_sink(), ctx.job_id, ctx.case_id, ctx.ds.map(|source| &source.id), 96,
        format!("Source facts: phase=source-facts stage={stage} state={state} elapsedMs={} rows={objects}", elapsed.as_millis()),
        ctx.cancel_requested(),
    );
}
