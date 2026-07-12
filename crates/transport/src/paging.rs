use serde::{Deserialize, Serialize};

/// Pagination request parameters.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PageRequest {
    /// Number of items to skip.
    pub offset: u64,
    /// Maximum number of items to return.
    pub limit: u32,
}

impl PageRequest {
    /// Maximum allowed page size to prevent memory exhaustion.
    pub const MAX_LIMIT: u32 = 500;

    /// Default page size.
    pub const DEFAULT_LIMIT: u32 = 100;

    /// Clamp the limit to the maximum allowed value.
    pub fn clamp(&mut self) {
        if self.limit == 0 {
            self.limit = Self::DEFAULT_LIMIT;
        }
        self.limit = self.limit.min(Self::MAX_LIMIT);
    }
}

#[cfg(test)]
#[path = "../tests/unit/paging.rs"]
mod tests;

/// Paginated response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResponse<T> {
    /// Total number of items available.
    pub total: u64,
    /// Items for the current page.
    pub items: Vec<T>,
}
