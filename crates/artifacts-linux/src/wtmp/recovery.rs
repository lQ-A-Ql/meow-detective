use super::*;

/// Recover a uniquely aligned complete suffix using the existing utmp layouts.
pub fn recover_wtmp_suffix(
    data: &[u8],
) -> Result<(usize, Vec<LoginRecord>), crate::LinuxArtifactError> {
    let mut accepted = None;
    for offset in 1..data.len().min(WTMP_SIZE_64) {
        let suffix = &data[offset..];
        let layouts = [layout_32(), layout_musl(), layout_64()];
        let mut valid = layouts.iter().filter(|layout| {
            !suffix.is_empty()
                && suffix.len().is_multiple_of(layout.record_size)
                && content_score(suffix, layout).accepts()
                && suffix.chunks_exact(layout.record_size).all(|bytes| {
                    read_utmp_record(bytes, layout).is_some_and(|record| {
                        record_is_padding(&record) || record_is_plausible(&record)
                    })
                })
        });
        if valid.next().is_none() {
            continue;
        }
        if valid.next().is_some() || accepted.is_some() {
            return Err(crate::LinuxArtifactError::ParseError {
                parser: "wtmp",
                message: "damaged wtmp has ambiguous record alignment".into(),
            });
        }
        let records = parse_wtmp(suffix)?;
        if !records.is_empty() {
            accepted = Some((offset, records));
        }
    }
    accepted.ok_or_else(|| crate::LinuxArtifactError::ParseError {
        parser: "wtmp",
        message: "damaged wtmp has no uniquely validated complete suffix".into(),
    })
}
