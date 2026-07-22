use crate::analysis_service::error::AnalysisServiceError;
use crate::analysis_service::extraction::artifact_query::{
    count_artifacts_by_type, query_artifact_rows, status_from_total, AnalysisArtifactRow,
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
    let totals = BrowserTotals::load(conn)?;
    let visits = map_visits(query_artifact_rows(
        conn,
        &["BrowserHistory"],
        offset,
        limit,
    )?);
    let downloads = map_downloads(query_artifact_rows(
        conn,
        &["BrowserDownload"],
        offset,
        limit,
    )?);
    let cookies = map_cookies(query_artifact_rows(
        conn,
        &["BrowserCookie"],
        offset,
        limit,
    )?);
    let sessions = map_sessions(query_artifact_rows(
        conn,
        &["BrowserSessionTab"],
        offset,
        limit,
    )?);
    let passwords = map_passwords(query_artifact_rows(
        conn,
        &["BrowserPassword"],
        offset,
        limit,
    )?);
    Ok(BrowserHistorySummaryDto {
        status: status_from_total(totals.total()),
        visit_total: totals.visits,
        download_total: totals.downloads,
        cookie_total: totals.cookies,
        session_total: totals.sessions,
        password_total: totals.passwords,
        visits,
        downloads,
        cookies,
        sessions,
        passwords,
        generated_at: Utc::now().to_rfc3339(),
        warnings: Vec::new(),
    })
}

struct BrowserTotals {
    visits: u64,
    downloads: u64,
    cookies: u64,
    sessions: u64,
    passwords: u64,
}

impl BrowserTotals {
    fn load(conn: &Connection) -> Result<Self, AnalysisServiceError> {
        Ok(Self {
            visits: count_artifacts_by_type(conn, "BrowserHistory")?,
            downloads: count_artifacts_by_type(conn, "BrowserDownload")?,
            cookies: count_artifacts_by_type(conn, "BrowserCookie")?,
            sessions: count_artifacts_by_type(conn, "BrowserSessionTab")?,
            passwords: count_artifacts_by_type(conn, "BrowserPassword")?,
        })
    }

    fn total(&self) -> u64 {
        self.visits + self.downloads + self.cookies + self.sessions + self.passwords
    }
}

fn map_visits(rows: Vec<AnalysisArtifactRow>) -> Vec<BrowserVisitDto> {
    rows.into_iter()
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
        .collect()
}

fn map_downloads(rows: Vec<AnalysisArtifactRow>) -> Vec<BrowserDownloadDto> {
    rows.into_iter()
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
        .collect()
}

fn map_cookies(rows: Vec<AnalysisArtifactRow>) -> Vec<BrowserCookieDto> {
    rows.into_iter()
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
            decryption_status: optional_string_attr(&row.attrs, "decryptionStatus"),
            decryption_detail: optional_string_attr(&row.attrs, "decryptionDetail"),
        })
        .collect()
}

fn map_sessions(rows: Vec<AnalysisArtifactRow>) -> Vec<BrowserSessionTabDto> {
    rows.into_iter()
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
        .collect()
}

fn map_passwords(rows: Vec<AnalysisArtifactRow>) -> Vec<BrowserPasswordDto> {
    rows.into_iter()
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
            decryption_status: optional_string_attr(&row.attrs, "decryptionStatus"),
            decryption_detail: optional_string_attr(&row.attrs, "decryptionDetail"),
        })
        .collect()
}
