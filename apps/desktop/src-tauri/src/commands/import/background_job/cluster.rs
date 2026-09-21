use super::{
    cluster_members::import_evidence_set_members,
    cluster_output::build_derived_processing_job,
    cluster_presence::assess_cephfs_presence,
    cluster_status::materialize_ceph_scope_rbd_sources,
    gate::acquire_import_slot,
    status::{cancel_job, fail_job, fail_linux_evidence_set_job},
    types::{
        BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport, EvidenceSetImportSummary,
    },
};
use crate::events::event_bridge;
use app_services::cluster_service;
use persistence_sqlite::repositories::job_repo::JobRepo;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::AppHandle;
use transport::CommandError;

pub(crate) fn run_background_linux_evidence_set_import_until_browseable(
    job: BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    let connection = app_services::connection::open_case_db(&job.db_path)
        .map_err(CommandError::from_typed_service_error)?;
    let job_repo = JobRepo::new(&connection);
    if let Some(app) = app {
        event_bridge::emit_job_started(app, &job.job_id.0, "Linux evidence-set import started");
        event_bridge::emit_job_progress(app, &job.job_id.0, 5, "Linux evidence-set import started");
    }
    if cancel_token.load(Ordering::Relaxed) {
        cancel_job(
            &job_repo,
            &job.job_id,
            app,
            "Linux evidence-set import cancelled by user",
        );
        return Ok(None);
    }
    let _import_slot = acquire_import_slot(&job_repo, &job.job_id, app, &cancel_token)?;
    initialize_evidence_set(&connection, &job_repo, &job, app)?;
    let Some(summary) =
        import_evidence_set_members(&connection, &job_repo, &job, app, &cancel_token)?
    else {
        return Ok(None);
    };
    complete_evidence_set_import(&connection, &job_repo, &job, app, summary, cancel_token)
}

fn initialize_evidence_set(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
) -> Result<(), CommandError> {
    if let Err(error) =
        cluster_service::register_linux_evidence_set_import(connection, &job.case_id, &job.plan)
    {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            None,
            CommandError::from_typed_service_error(error),
        );
    }
    if let Err(error) =
        cluster_service::write_linux_evidence_set_manifest(&job.case_root, &job.plan)
    {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((connection, &job.plan.import_set_id, 0, 1)),
            CommandError::from_typed_service_error(error),
        );
    }
    if let Err(error) = cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "importing",
        0,
        0,
        None,
    ) {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((connection, &job.plan.import_set_id, 0, 1)),
            CommandError::from_typed_service_error(error),
        );
    }
    Ok(())
}

fn complete_evidence_set_import(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: EvidenceSetImportSummary,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    let total_members = job.plan.members.len() as u32;
    if summary.failed_count > 0 {
        return complete_evidence_set_import_with_failures(
            connection,
            job_repo,
            job,
            app,
            summary,
            total_members,
        )
        .map(|()| None);
    }
    cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "ready",
        summary.ready_count,
        summary.failed_count,
        None,
    )
    .map_err(CommandError::from_typed_service_error)?;
    let topology = app_services::cluster_service::project_import_set_topology(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan.import_set_id,
    )
    .map_err(CommandError::from_typed_service_error)?;
    let Some(ceph_scope_id) = topology.ceph_scope_id else {
        return Ok(Some(BrowseableEvidenceSetImport {
            processing: build_derived_processing_job(job, Vec::new()),
            parent_job_id: job.job_id.clone(),
            completion_detail: format!(
                "Imported Linux evidence set {}: {}/{} source(s) ready; no Ceph scope proven",
                job.plan.import_set_name, summary.ready_count, total_members
            ),
        }));
    };
    let ceph_scope_id = domain::CephScopeId(ceph_scope_id);
    assess_cephfs_presence(connection, job, &ceph_scope_id);
    let Some(derived_sources) = materialize_ceph_scope_rbd_sources(
        connection,
        job_repo,
        job,
        app,
        &summary,
        &ceph_scope_id,
        Arc::clone(&cancel_token),
    )?
    else {
        return Ok(None);
    };
    if cancel_token.load(Ordering::Relaxed) {
        cancel_job(
            job_repo,
            &job.job_id,
            app,
            "Linux evidence-set import cancelled after RBD materialization",
        );
        return Ok(None);
    }
    let derived_source_count = derived_sources.len();
    let completion_detail = format!(
        "Imported Linux evidence set {}: {}/{} image(s) ready",
        job.plan.import_set_name, summary.ready_count, total_members
    );
    tracing::info!(
        import_set_id = %job.plan.import_set_id,
        members = total_members,
        imported = summary.ready_count,
        derived_sources = derived_source_count,
        summaries = ?summary.member_messages,
        "Linux evidence-set import is browseable and awaiting derived-task admission"
    );
    Ok(Some(BrowseableEvidenceSetImport {
        processing: build_derived_processing_job(job, derived_sources),
        parent_job_id: job.job_id.clone(),
        completion_detail,
    }))
}

fn complete_evidence_set_import_with_failures(
    connection: &rusqlite::Connection,
    job_repo: &JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: EvidenceSetImportSummary,
    total_members: u32,
) -> Result<(), CommandError> {
    let message = format!(
        "Linux evidence-set import finished with failures: {}/{} image(s) ready, {} failed",
        summary.ready_count, total_members, summary.failed_count
    );
    cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "failed",
        summary.ready_count,
        summary.failed_count,
        Some(&message),
    )
    .map_err(CommandError::from_typed_service_error)?;
    job_repo
        .update_outcome_counts(&job.job_id, 0, 0, summary.failed_count, true)
        .map_err(CommandError::from_typed_service_error)?;
    tracing::warn!(
        import_set_id = %job.plan.import_set_id,
        members = total_members,
        imported = summary.ready_count,
        failed = summary.failed_count,
        summaries = ?summary.member_messages,
        "Linux evidence-set import completed with member failures"
    );
    fail_job(job_repo, &job.job_id, app, CommandError::internal(message))
}
