use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_type, query_artifact_rows, status_from_total,
};
use crate::analysis_service::extraction::attr_mapping::{
    bool_attr, i32_attr, optional_i64_attr, optional_string_attr, string_attr, u64_attr,
};
use chrono::Utc;
use rusqlite::Connection;
use transport::dto::{
    BrowserCookieDto, BrowserDownloadDto, BrowserHistorySummaryDto, BrowserPasswordDto,
    BrowserSessionTabDto, BrowserVisitDto,
};

pub fn get_browser_history_summary(
    conn: &Connection,
    offset: u64,
    limit: u32,
) -> Result<BrowserHistorySummaryDto, AnalysisServiceError> {
    let visit_total = count_artifacts_by_type(conn, "BrowserHistory")?;
    let download_total = count_artifacts_by_type(conn, "BrowserDownload")?;
    let cookie_total = count_artifacts_by_type(conn, "BrowserCookie")?;
    let session_total = count_artifacts_by_type(conn, "BrowserSessionTab")?;
    let password_total = count_artifacts_by_type(conn, "BrowserPassword")?;
    let visit_rows = query_artifact_rows(conn, &["BrowserHistory"], offset, limit)?;
    let download_rows = query_artifact_rows(conn, &["BrowserDownload"], offset, limit)?;
    let cookie_rows = query_artifact_rows(conn, &["BrowserCookie"], offset, limit)?;
    let session_rows = query_artifact_rows(conn, &["BrowserSessionTab"], offset, limit)?;
    let password_rows = query_artifact_rows(conn, &["BrowserPassword"], offset, limit)?;
    let visits = visit_rows
        .into_iter()
        .map(|row| BrowserVisitDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            title: string_attr(&row.attrs, "title"),
            visit_time: optional_string_attr(&row.attrs, "visitTime"),
            visit_count: u64_attr(&row.attrs, "visitCount"),
        })
        .collect::<Vec<_>>();
    let downloads = download_rows
        .into_iter()
        .map(|row| BrowserDownloadDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            target_path: string_attr(&row.attrs, "targetPath"),
            start_time: optional_string_attr(&row.attrs, "startTime"),
            total_bytes: u64_attr(&row.attrs, "totalBytes"),
        })
        .collect::<Vec<_>>();
    let cookies = cookie_rows
        .into_iter()
        .map(|row| BrowserCookieDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: optional_string_attr(&row.attrs, "profile"),
            domain: string_attr(&row.attrs, "domain"),
            name: string_attr(&row.attrs, "name"),
            value_preview: optional_string_attr(&row.attrs, "valuePreview"),
            expiry: optional_string_attr(&row.attrs, "expiry"),
            secure: bool_attr(&row.attrs, "secure"),
            http_only: bool_attr(&row.attrs, "httpOnly"),
            same_site: optional_i64_attr(&row.attrs, "sameSite"),
        })
        .collect::<Vec<_>>();
    let sessions = session_rows
        .into_iter()
        .map(|row| BrowserSessionTabDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: optional_string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            title: optional_string_attr(&row.attrs, "title"),
            window_index: i32_attr(&row.attrs, "windowIndex"),
            tab_index: i32_attr(&row.attrs, "tabIndex"),
            last_active: optional_string_attr(&row.attrs, "lastActive"),
        })
        .collect::<Vec<_>>();
    let passwords = password_rows
        .into_iter()
        .map(|row| BrowserPasswordDto {
            artifact_id: row.id,
            file_id: row.source_object_id.unwrap_or_default(),
            source_path: string_attr(&row.attrs, "sourcePath"),
            browser: string_attr(&row.attrs, "browser"),
            profile: optional_string_attr(&row.attrs, "profile"),
            url: string_attr(&row.attrs, "url"),
            username: string_attr(&row.attrs, "username"),
            password_preview: optional_string_attr(&row.attrs, "passwordPreview"),
            created_at: optional_string_attr(&row.attrs, "createdAt"),
            times_used: u64_attr(&row.attrs, "timesUsed"),
        })
        .collect::<Vec<_>>();
    Ok(BrowserHistorySummaryDto {
        status: status_from_total(
            visit_total + download_total + cookie_total + session_total + password_total,
        ),
        visit_total,
        download_total,
        cookie_total,
        session_total,
        password_total,
        visits,
        downloads,
        cookies,
        sessions,
        passwords,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}
