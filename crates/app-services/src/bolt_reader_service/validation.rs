use super::BoltReadError;
use transport::dto::bolt_reader::{BoltReadReportDto, BoltReadRequestDto};

pub(super) fn report(
    request: &BoltReadRequestDto,
    report: &BoltReadReportDto,
) -> Result<(), BoltReadError> {
    if !report.metadata_checksum_verified
        || !report.page_size.is_power_of_two()
        || !(512..=65_536).contains(&report.page_size)
        || report.visited_entries > request.limits.max_entries as u64
        || report.visited_buckets > report.visited_entries
        || report.entries.len() as u64 > report.visited_entries
        || report
            .missing_bucket
            .is_some_and(|index| index as usize >= request.prefix.len())
    {
        return Err(BoltReadError::InvalidResponse);
    }
    for entry in &report.entries {
        let depth = if entry.is_bucket {
            request.bucket_parent_depth
        } else {
            request.parent_depth
        };
        if entry.bucket_path.len() > request.limits.max_depth as usize
            || !entry.bucket_path.starts_with(&request.prefix)
            || depth.is_some_and(|depth| {
                entry.bucket_path.len() != request.prefix.len() + depth as usize
            })
            || (entry.is_bucket
                && (!request.include_buckets
                    || entry.value.is_some()
                    || entry.value_length.is_some()))
            || (!entry.is_bucket && entry.value_length.is_none())
            || (!entry.is_bucket && !entry.bucket_path.ends_with(&request.parent_suffix))
            || (!entry.is_bucket
                && !request.record_keys.is_empty()
                && !request.record_keys.contains(&entry.key))
            || (entry.value.is_some() && !request.value_keys.contains(&entry.key))
            || entry
                .value
                .as_ref()
                .is_some_and(|value| Some(value.len() as u64) != entry.value_length)
        {
            return Err(BoltReadError::InvalidResponse);
        }
    }
    Ok(())
}

pub(super) fn retained_bytes(report: &BoltReadReportDto) -> u64 {
    let mut bytes = std::mem::size_of_val(report) as u64
        + (report.entries.capacity()
            * std::mem::size_of::<transport::dto::bolt_reader::BoltReadEntryDto>())
            as u64;
    for entry in &report.entries {
        bytes += entry.key.capacity() as u64
            + entry.value.as_ref().map_or(0, |value| value.capacity()) as u64;
        bytes += (entry.bucket_path.capacity() * std::mem::size_of::<Vec<u8>>()) as u64;
        bytes += entry
            .bucket_path
            .iter()
            .map(|part| part.capacity() as u64)
            .sum::<u64>();
    }
    bytes
}
