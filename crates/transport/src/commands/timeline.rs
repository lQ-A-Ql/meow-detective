use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::validation::{DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};
use crate::paging::validate_opaque_cursor;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTimelineRequest {
    #[serde(default)]
    pub offset: u64,
    #[serde(default = "default_timeline_limit")]
    pub limit: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

impl GetTimelineRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.limit == 0 {
            self.limit = DEFAULT_PAGE_LIMIT;
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        normalize_boundary(&mut self.time_start, "timeStart")?;
        normalize_boundary(&mut self.time_end, "timeEnd")?;
        if let (Some(start), Some(end)) = (&self.time_start, &self.time_end) {
            if start > end {
                return Err("timeStart must be before or equal to timeEnd".to_string());
            }
        }
        if let Some(cursor) = self.cursor.as_deref() {
            validate_opaque_cursor(cursor).map_err(|error| error.to_string())?;
            if self.offset != 0 {
                return Err("offset must be zero when cursor is provided".to_string());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTimelineEventByIdRequest {
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetTimelineFacetsRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_type: Option<String>,
    #[serde(default = "default_bucket_count")]
    pub bucket_count: u32,
}

impl GetTimelineFacetsRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.bucket_count == 0 {
            self.bucket_count = default_bucket_count();
        }
        self.bucket_count = self.bucket_count.max(20);
        self.bucket_count = self.bucket_count.min(180);
        normalize_boundary(&mut self.time_start, "timeStart")?;
        normalize_boundary(&mut self.time_end, "timeEnd")?;
        if let (Some(start), Some(end)) = (&self.time_start, &self.time_end) {
            if start > end {
                return Err("timeStart must be before or equal to timeEnd".to_string());
            }
        }
        Ok(())
    }
}

impl GetTimelineEventByIdRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.event_id.trim().is_empty() {
            return Err("eventId is required".to_string());
        }
        Ok(())
    }
}

fn default_timeline_limit() -> u32 {
    100
}

fn default_bucket_count() -> u32 {
    60
}

fn normalize_boundary(value: &mut Option<String>, field: &str) -> Result<(), String> {
    let Some(value) = value.as_mut() else {
        return Ok(());
    };
    let timestamp = DateTime::parse_from_rfc3339(value)
        .map_err(|_| format!("{field} must be an RFC3339 timestamp"))?
        .with_timezone(&Utc);
    *value = timestamp.to_rfc3339();
    Ok(())
}
