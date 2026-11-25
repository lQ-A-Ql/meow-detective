//! Register platform artifact work before source publication.
use super::context::{ImportContentKind, ImportJobContext};
use transport::CommandError;

pub(super) fn queue_artifacts(
    ctx: &mut ImportJobContext<'_>,
    source: &domain::DataSource,
    connection: &rusqlite::Connection,
    message: String,
) -> Result<String, CommandError> {
    ensure_active(ctx)?;
    let lease = ctx
        .runtime
        .as_ref()
        .map(|runtime| runtime.publication_lease());
    let _fence = lease
        .as_ref()
        .map(|lease| lease.fence_connection(connection));
    let eligible = ctx.content_kind == ImportContentKind::Filesystem
        && ctx.import_config.import_set.is_none()
        && !ctx.options.defer_linux_artifacts
        && matches!(
            ctx.import_config.platform,
            domain::DataSourcePlatform::Windows
                | domain::DataSourcePlatform::Linux
                | domain::DataSourcePlatform::Android
        );
    let (state, detail) = if eligible {
        (
            super::capability::QUEUED,
            "queued for post-import artifact extraction",
        )
    } else {
        (
            super::capability::NOT_APPLICABLE,
            "artifact extraction is not applicable for this source",
        )
    };
    super::capability::mark_capability(
        ctx.conn,
        ctx.case_id,
        &source.id,
        "artifacts",
        state,
        Some(detail),
        None,
    )?;
    ensure_active(ctx)?;
    Ok(message)
}

fn ensure_active(ctx: &ImportJobContext<'_>) -> Result<(), CommandError> {
    if let Some(profiler) = &ctx.profiler {
        profiler
            .check_output()
            .map_err(CommandError::from_typed_service_error)?;
    }
    if ctx.cancel_requested() {
        return Err(CommandError::cancelled("Import cancelled by user"));
    }
    super::runtime::validate_current(ctx.runtime.as_deref())
}

pub fn artifact_extraction_categories(
    platform: domain::DataSourcePlatform,
) -> Option<&'static [&'static str]> {
    match platform {
        domain::DataSourcePlatform::Windows => Some(&[
            "Registry",
            "BrowserHistory",
            "Email",
            "EventLogs",
            "ProgramExecution",
            "UserActivity",
            "RecycleBin",
            "Thumbnails",
            "ResourceUsage",
            "PluginArtifacts",
        ]),
        domain::DataSourcePlatform::Linux => Some(&["LinuxArtifacts"]),
        domain::DataSourcePlatform::Android | domain::DataSourcePlatform::Unknown => None,
    }
}
