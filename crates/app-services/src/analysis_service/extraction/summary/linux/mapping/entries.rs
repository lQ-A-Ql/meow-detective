use super::*;

impl LinuxEntries {
    pub(crate) fn load_page(
        conn: &Connection,
        offset: u64,
        limit: u32,
    ) -> Result<Self, AnalysisServiceError> {
        let mut pages = BTreeMap::new();
        pages.insert(
            LinuxSummaryFamily::Journal,
            query_artifact_rows_created_window(conn, "LinuxJournal", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::Login,
            query_artifact_rows_created_window(conn, "LinuxWtmp", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::BashCommand,
            query_artifact_rows_created_window(conn, "LinuxBashCommand", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::AptEvent,
            query_artifact_rows_created_window(conn, "LinuxAptEvent", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::CronJob,
            query_artifact_rows_created_window(conn, "LinuxCronJob", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::SudoEvent,
            query_artifact_rows_created_window(conn, "LinuxSudoEvent", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::SystemConfig,
            query_artifact_rows_created_window(conn, "LinuxSystemConfig", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::WebSite,
            query_artifact_rows_created_window(conn, "LinuxWebSite", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::WebAccessLog,
            query_artifact_rows_created_window(conn, "LinuxWebAccessLog", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::WebErrorLog,
            query_artifact_rows_created_window(conn, "LinuxWebErrorLog", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::WebFinding,
            query_artifact_rows_created_window(conn, "LinuxWebFinding", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::MysqlConfig,
            query_artifact_rows_created_window(conn, "LinuxMysqlConfig", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::MysqlLog,
            query_artifact_rows_created_window(conn, "LinuxMysqlLogEntry", offset, limit)?,
        );
        pages.insert(
            LinuxSummaryFamily::MysqlFinding,
            query_artifact_rows_created_window(conn, "LinuxMysqlFinding", offset, limit)?,
        );
        Ok(Self::from_pages(&mut pages))
    }

    pub(crate) fn from_pages(
        pages: &mut BTreeMap<LinuxSummaryFamily, Vec<AnalysisArtifactRow>>,
    ) -> Self {
        Self {
            journal: map_journal(
                pages
                    .remove(&LinuxSummaryFamily::Journal)
                    .unwrap_or_default(),
            ),
            login: map_logins(pages.remove(&LinuxSummaryFamily::Login).unwrap_or_default()),
            bash_commands: map_bash_commands(
                pages
                    .remove(&LinuxSummaryFamily::BashCommand)
                    .unwrap_or_default(),
            ),
            apt_events: map_apt_events(
                pages
                    .remove(&LinuxSummaryFamily::AptEvent)
                    .unwrap_or_default(),
            ),
            cron_jobs: map_cron_jobs(
                pages
                    .remove(&LinuxSummaryFamily::CronJob)
                    .unwrap_or_default(),
            ),
            sudo_events: map_sudo_events(
                pages
                    .remove(&LinuxSummaryFamily::SudoEvent)
                    .unwrap_or_default(),
            ),
            system_configs: map_system_configs(
                pages
                    .remove(&LinuxSummaryFamily::SystemConfig)
                    .unwrap_or_default(),
            ),
            web_sites: map_web_sites(
                pages
                    .remove(&LinuxSummaryFamily::WebSite)
                    .unwrap_or_default(),
            ),
            web_access_logs: map_web_access_logs(
                pages
                    .remove(&LinuxSummaryFamily::WebAccessLog)
                    .unwrap_or_default(),
            ),
            web_error_logs: map_web_error_logs(
                pages
                    .remove(&LinuxSummaryFamily::WebErrorLog)
                    .unwrap_or_default(),
            ),
            web_findings: map_web_findings(
                pages
                    .remove(&LinuxSummaryFamily::WebFinding)
                    .unwrap_or_default(),
            ),
            mysql_configs: map_mysql_configs(
                pages
                    .remove(&LinuxSummaryFamily::MysqlConfig)
                    .unwrap_or_default(),
            ),
            mysql_logs: map_mysql_logs(
                pages
                    .remove(&LinuxSummaryFamily::MysqlLog)
                    .unwrap_or_default(),
            ),
            mysql_findings: map_mysql_findings(
                pages
                    .remove(&LinuxSummaryFamily::MysqlFinding)
                    .unwrap_or_default(),
            ),
        }
    }
}
