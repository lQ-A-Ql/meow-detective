use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_linux_locked_shadow_accounts, count_linux_system_config_by_kind,
    count_linux_user_accounts, query_linux_hostname_rows, query_linux_system_config_by_kind,
};
use crate::analysis_service::extraction::attr_mapping::optional_string_attr;
use rusqlite::Connection;
use transport::dto::LinuxSystemInfoDto;

/// Build the unpaged host overview: os-release identity, `/etc/hostname`, and
/// passwd/shadow account statistics. Derived from dedicated small queries so
/// it stays correct regardless of the summary entry paging window.
pub(super) fn load_linux_system_info(
    conn: &Connection,
) -> Result<Option<LinuxSystemInfoDto>, AnalysisServiceError> {
    // Each source emits at most one os-release record; the first one wins.
    let os_release = query_linux_system_config_by_kind(conn, "osRelease", 1)?
        .into_iter()
        .next();
    let (os_pretty_name, os_id, os_version_id) = match os_release {
        Some(row) => (
            optional_string_attr(&row.attrs, "prettyName"),
            optional_string_attr(&row.attrs, "osId"),
            optional_string_attr(&row.attrs, "versionId"),
        ),
        None => (None, None, None),
    };

    // Hostname rows are generic text-config records; the first non-empty line
    // of the file is the hostname.
    let hostname = query_linux_hostname_rows(conn, 4)?
        .iter()
        .filter_map(|row| optional_string_attr(&row.attrs, "line"))
        .map(|line| line.trim().to_string())
        .find(|line| !line.is_empty());

    let account_count = count_linux_system_config_by_kind(conn, "passwdAccount")?;
    let user_account_count = count_linux_user_accounts(conn)?;
    let locked_account_count = count_linux_locked_shadow_accounts(conn)?;

    let has_identity = os_pretty_name.is_some()
        || os_id.is_some()
        || os_version_id.is_some()
        || hostname.is_some();
    if !has_identity && account_count == 0 && locked_account_count == 0 {
        return Ok(None);
    }

    Ok(Some(LinuxSystemInfoDto {
        os_pretty_name,
        os_id,
        os_version_id,
        hostname,
        account_count,
        user_account_count,
        locked_account_count,
    }))
}
