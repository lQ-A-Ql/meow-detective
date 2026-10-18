//! Bounded ZIP local-file/central-directory validation for carving.
use super::{CarveCandidate, CarveError, CarveStatus, ScanRequest, ZIP_SIGNATURE};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::atomic::Ordering,
};

const LOCAL: &[u8; 4] = b"PK\x03\x04";
const CENTRAL: &[u8; 4] = b"PK\x01\x02";
const END: &[u8; 4] = b"PK\x05\x06";
const LOCAL_SIZE: usize = 30;
const CENTRAL_SIZE: usize = 46;

pub(super) fn validate_zip<R: Read + Seek + ?Sized>(
    reader: &mut R,
    request: &ScanRequest<'_>,
    offset: u64,
) -> Result<CarveCandidate, CarveError> {
    let limit = request
        .scope
        .end()?
        .min(offset.saturating_add(request.limits.max_object_bytes));
    reader.seek(SeekFrom::Start(offset))?;
    let mut input = ZipInput {
        reader,
        at: offset,
        end: limit,
        budget: request.limits.validation_bytes,
        hash: Sha256::new(),
        cancel: request.cancel,
    };
    let result = parse_archive(&mut input, request.limits.max_chunks);
    let mut candidate = CarveCandidate {
        source_id: request.scope.source_id.clone(),
        revision: request.scope.revision.clone(),
        view: request.scope.view.clone(),
        offset,
        length: None,
        sha256: None,
        status: CarveStatus::Rejected,
        reason: "invalidZip",
        validator_version: super::ZIP_VALIDATOR_VERSION,
    };
    match result {
        Ok(()) => {
            candidate.length = Some(input.at - offset);
            candidate.sha256 = Some(format!("{:x}", input.hash.finalize()));
            candidate.status = CarveStatus::Complete;
            candidate.reason = "verifiedZipStructure";
        }
        Err(ZipError::Invalid(reason)) => candidate.reason = reason,
        Err(ZipError::Truncated) => {
            candidate.status = CarveStatus::Partial;
            candidate.reason = "truncatedZip";
        }
        Err(ZipError::Core(error)) => return Err(error),
    }
    Ok(candidate)
}

enum ZipError {
    Invalid(&'static str),
    Truncated,
    Core(CarveError),
}

struct ZipInput<'a, R: ?Sized> {
    reader: &'a mut R,
    at: u64,
    end: u64,
    budget: u64,
    hash: Sha256,
    cancel: &'a std::sync::atomic::AtomicBool,
}
impl<R: Read + ?Sized> ZipInput<'_, R> {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), ZipError> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(ZipError::Core(CarveError::Cancelled));
        }
        if bytes.len() as u64 > self.end.saturating_sub(self.at) {
            return Err(ZipError::Truncated);
        }
        if bytes.len() as u64 > self.budget {
            return Err(ZipError::Core(CarveError::Limit));
        }
        self.reader.read_exact(bytes).map_err(|error| {
            if error.kind() == std::io::ErrorKind::UnexpectedEof {
                ZipError::Truncated
            } else {
                ZipError::Core(CarveError::Io(error))
            }
        })?;
        self.at += bytes.len() as u64;
        self.budget -= bytes.len() as u64;
        self.hash.update(bytes);
        Ok(())
    }
    fn skip(&mut self, mut bytes: u64) -> Result<(), ZipError> {
        let mut buffer = [0u8; 64 * 1024];
        while bytes > 0 {
            let amount = bytes.min(buffer.len() as u64) as usize;
            self.read(&mut buffer[..amount])?;
            bytes -= amount as u64;
        }
        Ok(())
    }
}

fn parse_archive<R: Read + ?Sized>(
    input: &mut ZipInput<'_, R>,
    max_entries: u32,
) -> Result<(), ZipError> {
    let archive_start = input.at;
    let mut local_count = 0u32;
    let (local_end, first_directory_record) = loop {
        if local_count >= max_entries {
            return Err(ZipError::Invalid("entryCountLimit"));
        }
        let mut signature = [0u8; 4];
        input.read(&mut signature)?;
        if &signature == LOCAL {
            let mut header = [0u8; LOCAL_SIZE - 4];
            input.read(&mut header)?;
            let flags = u16::from_le_bytes([header[2], header[3]]);
            let method = u16::from_le_bytes([header[4], header[5]]);
            let compressed = u32::from_le_bytes(header[14..18].try_into().unwrap()) as u64;
            let inflated = u32::from_le_bytes(header[18..22].try_into().unwrap()) as u64;
            let name = u16::from_le_bytes(header[22..24].try_into().unwrap()) as u64;
            let extra = u16::from_le_bytes(header[24..26].try_into().unwrap()) as u64;
            if flags & 0x0009 != 0 || flags & 0x0008 != 0 {
                return Err(ZipError::Invalid("unsupportedZipFlags"));
            }
            if !matches!(method, 0 | 8) {
                return Err(ZipError::Invalid("unsupportedCompression"));
            }
            if compressed > 128 * 1024 * 1024 || inflated > 128 * 1024 * 1024 {
                return Err(ZipError::Invalid("entryLimit"));
            }
            input.skip(
                name.checked_add(extra)
                    .ok_or(ZipError::Invalid("headerOverflow"))?,
            )?;
            input.skip(compressed)?;
            local_count += 1;
        } else if &signature == CENTRAL || &signature == END {
            break (input.at - 4, signature);
        } else {
            return Err(ZipError::Invalid("invalidZipRecord"));
        }
    };
    let local_bytes = local_end - archive_start;
    let mut central_count = 0u32;
    let central_start = local_end;
    let mut next_record = first_directory_record;
    let end = loop {
        if central_count > max_entries {
            return Err(ZipError::Invalid("entryCountLimit"));
        }
        let signature = next_record;
        if &signature == CENTRAL {
            let mut header = [0u8; CENTRAL_SIZE - 4];
            input.read(&mut header)?;
            let compressed = u32::from_le_bytes(header[16..20].try_into().unwrap());
            let inflated = u32::from_le_bytes(header[20..24].try_into().unwrap());
            let name = u16::from_le_bytes([header[24], header[25]]) as u64;
            let extra = u16::from_le_bytes([header[26], header[27]]) as u64;
            let comment = u16::from_le_bytes([header[28], header[29]]) as u64;
            let local_offset = u32::from_le_bytes(header[38..42].try_into().unwrap()) as u64;
            if compressed == u32::MAX || inflated == u32::MAX || local_offset >= local_bytes {
                return Err(ZipError::Invalid("unsupportedZip64"));
            }
            input.skip(
                name.checked_add(extra)
                    .and_then(|v| v.checked_add(comment))
                    .ok_or(ZipError::Invalid("centralOverflow"))?,
            )?;
            central_count += 1;
        } else if &signature == END {
            break input.at - 4;
        } else {
            return Err(ZipError::Invalid("invalidCentralDirectory"));
        }
        input.read(&mut next_record)?;
    };
    let mut trailer = [0u8; 18];
    input.read(&mut trailer)?;
    let disk = u16::from_le_bytes([trailer[0], trailer[1]]);
    let central_disk = u16::from_le_bytes([trailer[2], trailer[3]]);
    let disk_entries = u16::from_le_bytes([trailer[4], trailer[5]]) as u32;
    let total_entries = u16::from_le_bytes([trailer[6], trailer[7]]) as u32;
    let central_size = u32::from_le_bytes(trailer[8..12].try_into().unwrap()) as u64;
    let central_offset = u32::from_le_bytes(trailer[12..16].try_into().unwrap()) as u64;
    let comment_len = u16::from_le_bytes([trailer[16], trailer[17]]) as u64;
    input.skip(comment_len)?;
    if disk != 0
        || central_disk != 0
        || disk_entries != total_entries
        || total_entries != central_count
        || central_offset != local_bytes
        || central_size != end - central_start
    {
        return Err(ZipError::Invalid("inconsistentZipDirectory"));
    }
    Ok(())
}

pub fn scan_zip<R: Read + Seek + ?Sized>(
    reader: &mut R,
    request: ScanRequest<'_>,
    mut sink: impl FnMut(CarveCandidate) -> Result<(), CarveError>,
) -> Result<super::ScanReport, CarveError> {
    let end = request.scope.end()?;
    request.limits.validate()?;
    let mut buffer = vec![0; request.limits.window_bytes];
    let mut at = request.scope.offset;
    let mut last_candidate = None;
    let mut report = super::ScanReport::default();
    while at < end {
        if request.cancel.load(Ordering::Acquire) {
            return Err(CarveError::Cancelled);
        }
        let count = (end - at).min(buffer.len() as u64) as usize;
        reader.seek(SeekFrom::Start(at))?;
        reader.read_exact(&mut buffer[..count])?;
        for (index, bytes) in buffer[..count].windows(ZIP_SIGNATURE.len()).enumerate() {
            if bytes != ZIP_SIGNATURE {
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
            let candidate = validate_zip(reader, &request, offset)?;
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
        at += (count - (ZIP_SIGNATURE.len() - 1)) as u64;
    }
    Ok(report)
}
