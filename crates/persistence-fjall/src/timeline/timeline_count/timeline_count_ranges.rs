use super::super::{decode_record, record_key};
use super::read_count;
use crate::{
    codec::{encode_key, KeyPart, VersionedEnvelope},
    database::SourceReadSnapshot,
    errors::{FjallStorageError, Result},
    keyspaces::KeyspaceName,
    scan::{ScanBounds, ScanDirection},
};
use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc};
use std::ops::Bound;

const TIME_FAMILY: &str = "timeline.time";
const TYPE_FAMILY: &str = "timeline.type";
const INDEX_PAGE_SIZE: usize = 512;

pub(super) fn count_interval(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<u64> {
    if start.date_naive() == end.date_naive() {
        return count_index_range(snapshot, source_id, event_type, start, end);
    }
    let start_date = start.date_naive();
    let end_date = end.date_naive();
    let mut count = 0u64;
    let first_full_day = if start.time() == NaiveTime::MIN {
        start_date
    } else {
        count += count_index_range(
            snapshot,
            source_id,
            event_type,
            start,
            day_end_utc(start_date)?,
        )?;
        start_date
            .succ_opt()
            .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)?
    };
    let last_full_day = if is_day_end(end) {
        end_date
    } else {
        count += count_index_range(
            snapshot,
            source_id,
            event_type,
            end_date.and_time(NaiveTime::MIN).and_utc(),
            end,
        )?;
        end_date
            .pred_opt()
            .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)?
    };
    if first_full_day <= last_full_day {
        count += count_full_days(
            snapshot,
            source_id,
            event_type,
            first_full_day,
            last_full_day,
        )?;
    }
    Ok(count)
}

fn is_day_end(timestamp: DateTime<Utc>) -> bool {
    timestamp.time()
        == NaiveTime::from_hms_nano_opt(23, 59, 59, 999_999_999).expect("valid end of day")
}

fn day_end_utc(date: NaiveDate) -> Result<DateTime<Utc>> {
    date.and_hms_nano_opt(23, 59, 59, 999_999_999)
        .map(|value| value.and_utc())
        .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)
}

fn count_full_days(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    mut first: NaiveDate,
    last: NaiveDate,
) -> Result<u64> {
    let mut count = 0u64;
    while first <= last {
        let year_end = NaiveDate::from_ymd_opt(first.year(), 12, 31)
            .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)?;
        let month_end = first
            .with_day(u32::from(first.num_days_in_month()))
            .ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)?;
        let (period, next) = if first.ordinal() == 1 && year_end <= last {
            (format!("year:{}", first.format("%Y")), year_end.succ_opt())
        } else if first.day() == 1 && month_end <= last {
            (
                format!("month:{}", first.format("%Y-%m")),
                month_end.succ_opt(),
            )
        } else {
            (
                format!("day:{}", first.format("%Y-%m-%d")),
                first.succ_opt(),
            )
        };
        count += read_count(snapshot, source_id, event_type, &period)?;
        first = next.ok_or(FjallStorageError::InvalidTimelineTimestampEncoding)?;
    }
    Ok(count)
}

fn count_index_range(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<u64> {
    if start > end {
        return Ok(0);
    }
    let family = event_type.map_or(TIME_FAMILY, |_| TYPE_FAMILY);
    let mut parts = vec![KeyPart::Text(source_id.to_owned())];
    if let Some(event_type) = event_type {
        parts.push(KeyPart::Text(event_type.to_owned()));
    }
    parts.push(KeyPart::InverseU64(timestamp_sort_value(end)));
    let lower = encode_key(family, &parts)?;
    parts.pop();
    parts.push(KeyPart::InverseU64(timestamp_sort_value(start)));
    let upper = prefix_end(&encode_key(family, &parts)?)?;
    let bounds = ScanBounds {
        start: Bound::Included(lower),
        end: Bound::Excluded(upper),
    };
    let mut count = 0u64;
    let mut cursor = None;
    loop {
        let entries = snapshot.range_keyset(
            KeyspaceName::TimelineIndexes,
            bounds.clone(),
            cursor.as_deref(),
            INDEX_PAGE_SIZE,
        )?;
        if entries.is_empty() {
            break;
        }
        for entry in &entries {
            let bytes = snapshot
                .get(KeyspaceName::TimelineRecords, &entry.value)?
                .ok_or(FjallStorageError::TimelineIndexCorrupt)?;
            let record = decode_record(VersionedEnvelope::decode(&bytes)?)?;
            if record_key(&record.data_source_id, &record.id)? != entry.value {
                return Err(FjallStorageError::TimelineRecordIdentityMismatch);
            }
            if record.timestamp >= start
                && record.timestamp <= end
                && event_type.is_none_or(|kind| record.event_type == kind)
            {
                count += 1;
            }
        }
        if entries.len() < INDEX_PAGE_SIZE {
            break;
        }
        cursor = entries.last().map(|entry| entry.key.clone());
    }
    Ok(count)
}

pub(super) fn legacy_count(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<u64> {
    if start.is_none() && end.is_none() {
        let prefix = index_prefix(source_id, event_type)?;
        return count_index_prefix(snapshot, &prefix, source_id, event_type, start, end);
    }
    let first = match start {
        Some(value) => value,
        None => edge_timestamp_for_count(snapshot, source_id, event_type, ScanDirection::Reverse)?
            .ok_or(FjallStorageError::TimelineIndexCorrupt)?,
    };
    let last = match end {
        Some(value) => value,
        None => edge_timestamp_for_count(snapshot, source_id, event_type, ScanDirection::Forward)?
            .ok_or(FjallStorageError::TimelineIndexCorrupt)?,
    };
    count_index_range(snapshot, source_id, event_type, first, last)
}

fn count_index_prefix(
    snapshot: &SourceReadSnapshot,
    prefix: &[u8],
    source_id: &str,
    event_type: Option<&str>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<u64> {
    let mut count = 0u64;
    let mut cursor = None;
    loop {
        let entries = snapshot.keyset(
            KeyspaceName::TimelineIndexes,
            prefix,
            cursor.as_deref(),
            ScanDirection::Forward,
            INDEX_PAGE_SIZE,
        )?;
        if entries.is_empty() {
            break;
        }
        for entry in &entries {
            let bytes = snapshot
                .get(KeyspaceName::TimelineRecords, &entry.value)?
                .ok_or(FjallStorageError::TimelineIndexCorrupt)?;
            let record = decode_record(VersionedEnvelope::decode(&bytes)?)?;
            if record.data_source_id != source_id
                || event_type.is_some_and(|kind| record.event_type != kind)
            {
                return Err(FjallStorageError::TimelineRecordIdentityMismatch);
            }
            if start.is_none_or(|bound| record.timestamp >= bound)
                && end.is_none_or(|bound| record.timestamp <= bound)
            {
                count += 1;
            }
        }
        if entries.len() < INDEX_PAGE_SIZE {
            break;
        }
        cursor = entries.last().map(|entry| entry.key.clone());
    }
    Ok(count)
}

pub(super) fn edge_timestamp_for_count(
    snapshot: &SourceReadSnapshot,
    source_id: &str,
    event_type: Option<&str>,
    direction: ScanDirection,
) -> Result<Option<DateTime<Utc>>> {
    let prefix = index_prefix(source_id, event_type)?;
    let Some(entry) = snapshot
        .keyset(KeyspaceName::TimelineIndexes, &prefix, None, direction, 1)?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let bytes = snapshot
        .get(KeyspaceName::TimelineRecords, &entry.value)?
        .ok_or(FjallStorageError::TimelineIndexCorrupt)?;
    Ok(Some(
        decode_record(VersionedEnvelope::decode(&bytes)?)?.timestamp,
    ))
}

fn index_prefix(source_id: &str, event_type: Option<&str>) -> Result<Vec<u8>> {
    match event_type {
        Some(kind) => encode_key(
            TYPE_FAMILY,
            &[
                KeyPart::Text(source_id.to_owned()),
                KeyPart::Text(kind.to_owned()),
            ],
        ),
        None => encode_key(TIME_FAMILY, &[KeyPart::Text(source_id.to_owned())]),
    }
}

fn prefix_end(prefix: &[u8]) -> Result<Vec<u8>> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.last_mut() {
        if *last == u8::MAX {
            end.pop();
        } else {
            *last += 1;
            return Ok(end);
        }
    }
    Err(FjallStorageError::InvalidEnvelope(
        "timeline time prefix has no upper bound".to_owned(),
    ))
}

fn timestamp_sort_value(timestamp: DateTime<Utc>) -> u64 {
    (timestamp.timestamp_micros() as u64) ^ (1 << 63)
}
