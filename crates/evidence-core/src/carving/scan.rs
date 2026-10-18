use super::{
    png::{validate_png, ValidationBudget},
    CarveCandidate, CarveError, CarveStatus, ScanReport, ScanRequest, PNG_SIGNATURE,
};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::atomic::Ordering,
};

pub fn scan_png<R: Read + Seek + ?Sized>(
    reader: &mut R,
    request: ScanRequest<'_>,
    mut sink: impl FnMut(CarveCandidate) -> Result<(), CarveError>,
) -> Result<ScanReport, CarveError> {
    let end = request.scope.end()?;
    request.limits.validate()?;
    let mut buffer = vec![0; request.limits.window_bytes];
    let mut at = request.scope.offset;
    let mut last_candidate = None;
    let mut report = ScanReport::default();
    let mut budget = ValidationBudget {
        input: request.limits.validation_bytes,
        output: request.limits.inflated_bytes,
    };
    while at < end {
        if request.cancel.load(Ordering::Acquire) {
            return Err(CarveError::Cancelled);
        }
        let count = (end - at).min(buffer.len() as u64) as usize;
        reader.seek(SeekFrom::Start(at))?;
        reader.read_exact(&mut buffer[..count])?;
        for (index, bytes) in buffer[..count].windows(PNG_SIGNATURE.len()).enumerate() {
            if bytes != PNG_SIGNATURE {
                continue;
            }
            let offset = at + index as u64;
            if last_candidate.is_some_and(|last| offset <= last) {
                continue;
            }
            if report.candidates == request.limits.max_candidates {
                return Err(CarveError::Limit);
            }
            last_candidate = Some(offset);
            let candidate = validate_png(reader, &request, offset, &mut budget)?;
            report.candidates += 1;
            match candidate.status {
                CarveStatus::Complete => report.complete += 1,
                CarveStatus::Partial => report.partial += 1,
                CarveStatus::Rejected => report.rejected += 1,
            }
            sink(candidate)?;
        }
        report.scanned_bytes = at + count as u64 - request.scope.offset;
        (request.progress)(report.scanned_bytes, request.scope.length);
        if count < buffer.len() || at + count as u64 == end {
            break;
        }
        at += (count - (PNG_SIGNATURE.len() - 1)) as u64;
    }
    Ok(report)
}
