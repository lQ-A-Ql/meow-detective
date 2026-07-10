#[derive(Debug, Clone, Copy, Default)]
pub struct TimelineQuery<'a> {
    pub offset: u64,
    pub limit: u32,
    pub time_start: Option<&'a str>,
    pub time_end: Option<&'a str>,
    pub event_type: Option<&'a str>,
}

impl TimelineQuery<'_> {
    pub const fn unfiltered(offset: u64, limit: u32) -> Self {
        Self {
            offset,
            limit,
            time_start: None,
            time_end: None,
            event_type: None,
        }
    }
}
