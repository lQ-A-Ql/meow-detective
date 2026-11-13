use transport::dto::TimelineHistogramBucketDto;

use super::format_epoch;
use crate::timeline_service::TimelineServiceError;

const MAX_BUCKET_COUNT: u32 = 180;

pub(super) struct Histogram {
    bounds: Option<(i64, i64)>,
    counts: Vec<u64>,
}

impl Histogram {
    pub(super) fn new(bounds: Option<(i64, i64)>, requested: u32) -> Self {
        let count = bounds.map_or(0, |(first, last)| {
            u64::from(requested.clamp(1, MAX_BUCKET_COUNT))
                .min(last.saturating_sub(first) as u64 + 1) as usize
        });
        Self {
            bounds,
            counts: vec![0; count],
        }
    }

    pub(super) fn windows(&self) -> Vec<(i64, i64)> {
        let Some((first, last)) = self.bounds else {
            return Vec::new();
        };
        let span = i128::from(last) - i128::from(first) + 1;
        let count = self.counts.len() as i128;
        (0..self.counts.len())
            .map(|index| {
                (
                    (i128::from(first) + span * index as i128 / count) as i64,
                    (i128::from(first) + span * (index as i128 + 1) / count - 1) as i64,
                )
            })
            .collect()
    }

    pub(super) fn add_counts(&mut self, counts: &[u64]) -> Result<(), TimelineServiceError> {
        if self.counts.len() != counts.len() {
            return Err(persistence_source::fjall::FjallStorageError::TimelineIndexCorrupt.into());
        }
        for (target, count) in self.counts.iter_mut().zip(counts) {
            *target = target.checked_add(*count).ok_or_else(|| {
                TimelineServiceError::Other("timeline histogram count overflow".to_owned())
            })?;
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<Vec<TimelineHistogramBucketDto>, TimelineServiceError> {
        let windows = self.windows();
        self.counts
            .into_iter()
            .zip(windows)
            .map(|(value, (start, end))| {
                Ok(TimelineHistogramBucketDto {
                    start_ts: format_epoch(start)?,
                    end_ts: format_epoch(end)?,
                    count: value,
                })
            })
            .collect()
    }
}
