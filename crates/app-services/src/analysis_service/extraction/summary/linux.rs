mod aggregate;
mod cache;
mod mapping;
mod system_info;

use self::aggregate::LinuxSummaryAggregate;
use self::cache::LinuxSummaryRevisions;
use self::mapping::{LinuxCounts, LinuxEntries};
use self::system_info::derive_linux_system_info;
use crate::analysis_service::candidates::collect_linux_artifact_catalog;
use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_linux_system_config_by_kind_paged, query_linux_system_config_by_kind_paged,
};
use crate::analysis_service::extraction::observability::{
    linux_summary_observability, linux_summary_status, LinuxSummaryObservability,
};
use chrono::Utc;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use transport::dto::{LinuxArtifactSummaryDto, LinuxSystemConfigPageDto, LinuxSystemInfoDto};

const SUMMARY_CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct LinuxSummaryStaticCache {
    schema_version: u32,
    artifact_revision: u64,
    analysis_revision: u64,
    counts: LinuxCounts,
    observability: LinuxSummaryObservability,
    system_info: Option<LinuxSystemInfoDto>,
    generated_at: String,
}

impl LinuxSummaryStaticCache {
    fn into_dto(self, entries: LinuxEntries) -> LinuxArtifactSummaryDto {
        let total_count = self.counts.total();
        LinuxArtifactSummaryDto {
            status: linux_summary_status(total_count, self.observability.candidate_count),
            journal_count: self.counts.journal,
            text_log_count: self.counts.text_log,
            login_count: self.counts.login,
            bash_command_count: self.counts.bash_command,
            apt_event_count: self.counts.apt_event,
            cron_job_count: self.counts.cron_job,
            sudo_event_count: self.counts.sudo_event,
            system_config_count: self.counts.system_config,
            web_site_count: self.counts.web_site,
            web_access_log_count: self.counts.web_access_log,
            web_error_log_count: self.counts.web_error_log,
            web_finding_count: self.counts.web_finding,
            mysql_config_count: self.counts.mysql_config,
            mysql_log_count: self.counts.mysql_log,
            mysql_finding_count: self.counts.mysql_finding,
            total_count,
            truncated: self.observability.truncated,
            coverage_ratio: self.observability.coverage_ratio,
            journal_entries: entries.journal,
            login_records: entries.login,
            bash_commands: entries.bash_commands,
            apt_events: entries.apt_events,
            cron_jobs: entries.cron_jobs,
            sudo_events: entries.sudo_events,
            system_configs: entries.system_configs,
            web_sites: entries.web_sites,
            web_access_logs: entries.web_access_logs,
            web_error_logs: entries.web_error_logs,
            web_findings: entries.web_findings,
            mysql_configs: entries.mysql_configs,
            mysql_logs: entries.mysql_logs,
            mysql_findings: entries.mysql_findings,
            system_info: self.system_info,
            generated_at: self.generated_at,
            warnings: self.observability.warnings,
            coverage_notes: self.observability.coverage_notes,
        }
    }
}

pub fn get_linux_system_config_page(
    conn: &Connection,
    config_kind: &str,
    offset: u64,
    limit: u32,
) -> Result<LinuxSystemConfigPageDto, AnalysisServiceError> {
    let total = count_linux_system_config_by_kind_paged(conn, config_kind)?;
    let rows = mapping::map_system_configs(query_linux_system_config_by_kind_paged(
        conn,
        config_kind,
        offset,
        limit,
    )?);
    Ok(LinuxSystemConfigPageDto {
        has_more: offset.saturating_add(rows.len() as u64) < total,
        rows,
        total,
        offset,
        limit,
    })
}

/// Get Linux artifact summary (systemd journal, text-log fallback lines,
/// wtmp/btmp logins, bash history, apt/dpkg/yum/dnf package events, cron jobs,
/// sudo/auth events, and system config records).
///
/// This is a case-wide aggregate, matching the existing Registry/Browser/EVTX
/// summary functions.
pub fn get_linux_artifact_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<LinuxArtifactSummaryDto, AnalysisServiceError> {
    let revisions = LinuxSummaryRevisions::load(conn)?;
    if let Some(summary) = cache::read_page(conn, revisions, offset, limit)? {
        tracing::debug!(
            offset,
            limit,
            "Linux artifact summary served from page cache"
        );
        return Ok(summary);
    }
    if let Some(snapshot) = cache::read_static::<LinuxSummaryStaticCache>(conn, revisions)? {
        let entries = LinuxEntries::load_page(conn, offset, limit)?;
        tracing::debug!(
            offset,
            limit,
            "Linux artifact summary entries loaded from family index"
        );
        return Ok(snapshot.into_dto(entries));
    }
    load_linux_artifact_summary_uncached(conn, revisions, offset, limit).map(|(_, summary)| summary)
}

pub(crate) fn warm_linux_artifact_summary(
    conn: &Connection,
    limit: u32,
) -> Result<LinuxArtifactSummaryDto, AnalysisServiceError> {
    let offset = 0;
    let revisions = LinuxSummaryRevisions::load(conn)?;
    let (snapshot, summary) = load_linux_artifact_summary_uncached(conn, revisions, offset, limit)?;
    cache::write_static(conn, revisions, &snapshot)?;
    cache::write_page(conn, revisions, offset, limit, &summary)?;
    Ok(summary)
}

fn load_linux_artifact_summary_uncached(
    conn: &Connection,
    revisions: LinuxSummaryRevisions,
    offset: u64,
    limit: u32,
) -> Result<(LinuxSummaryStaticCache, LinuxArtifactSummaryDto), AnalysisServiceError> {
    let started = Instant::now();
    let catalog = collect_linux_artifact_catalog(conn)?;
    let aggregate = LinuxSummaryAggregate::load(conn, offset, limit)?;
    let total_count = aggregate.counts.total();
    let system_info = derive_linux_system_info(&catalog, &aggregate.system_inputs);
    let observability =
        linux_summary_observability(&catalog, total_count, &aggregate.parsed_source_ids);
    tracing::info!(
        elapsed_ms = started.elapsed().as_millis() as u64,
        candidate_count = catalog.candidates.len(),
        artifact_count = total_count,
        offset,
        limit,
        "Linux artifact summary loaded"
    );
    let snapshot = LinuxSummaryStaticCache {
        schema_version: SUMMARY_CACHE_SCHEMA_VERSION,
        artifact_revision: revisions.artifact(),
        analysis_revision: revisions.analysis(),
        counts: aggregate.counts,
        observability,
        system_info,
        generated_at: Utc::now().to_rfc3339(),
    };
    let summary = snapshot.clone().into_dto(aggregate.entries);
    Ok((snapshot, summary))
}
