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

    pub(super) fn add(&mut self, timestamp: i64) -> Result<(), TimelineServiceError> {
        let Some((first, last)) = self.bounds else {
            return Err(TimelineServiceError::Storage(
                persistence_source::fjall::FjallStorageError::TimelineIndexCorrupt,
            ));
        };
        if timestamp < first || timestamp > last {
            return Err(TimelineServiceError::Storage(
                persistence_source::fjall::FjallStorageError::TimelineIndexCorrupt,
            ));
        }
        let span = i128::from(last) - i128::from(first) + 1;
        let position = i128::from(timestamp) - i128::from(first);
        let index = (((position + 1) * self.counts.len() as i128 - 1) / span) as usize;
        self.counts[index] = self.counts[index].checked_add(1).ok_or_else(|| {
            TimelineServiceError::Other("timeline histogram count overflow".to_owned())
        })?;
        Ok(())
    }

    pub(super) fn finish(self) -> Result<Vec<TimelineHistogramBucketDto>, TimelineServiceError> {
        let Some((first, last)) = self.bounds else {
            return Ok(Vec::new());
        };
        let span = i128::from(last) - i128::from(first) + 1;
        let count = self.counts.len() as i128;
        self.counts
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                let start = i128::from(first) + span * index as i128 / count;
                let end = (i128::from(first) + span * (index as i128 + 1) / count - 1)
                    .min(i128::from(last));
                Ok(TimelineHistogramBucketDto {
                    start_ts: format_epoch(i64::try_from(start).map_err(|_| {
                        TimelineServiceError::Other("timeline histogram start overflow".to_owned())
                    })?)?,
                    end_ts: format_epoch(i64::try_from(end).map_err(|_| {
                        TimelineServiceError::Other("timeline histogram end overflow".to_owned())
                    })?)?,
                    count: value,
                })
            })
            .collect()
    }
}
