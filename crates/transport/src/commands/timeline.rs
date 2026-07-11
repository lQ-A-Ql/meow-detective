use serde::{Deserialize, Serialize};

use super::validation::{DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};

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
}

impl GetTimelineRequest {
    pub fn validate(&mut self) -> Result<(), String> {
        if self.limit == 0 {
            self.limit = DEFAULT_PAGE_LIMIT;
        }
        self.limit = self.limit.min(MAX_PAGE_LIMIT);
        if let (Some(start), Some(end)) = (&self.time_start, &self.time_end) {
            if start > end {
                return Err("timeStart must be before or equal to timeEnd".to_string());
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
