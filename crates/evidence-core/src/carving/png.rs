use super::{
    png_stream::Raster, CarveCandidate, CarveError, CarveStatus, ScanRequest, PNG_SIGNATURE,
    PNG_VALIDATOR_VERSION,
};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Seek, SeekFrom},
    sync::atomic::Ordering,
};

pub(super) struct ValidationBudget {
    pub input: u64,
    pub output: u64,
}

pub(super) fn validate_png<R: Read + Seek + ?Sized>(
    reader: &mut R,
    request: &ScanRequest<'_>,
    offset: u64,
    budget: &mut ValidationBudget,
) -> Result<CarveCandidate, CarveError> {
    let mut candidate = CarveCandidate {
        source_id: request.scope.source_id.clone(),
        revision: request.scope.revision.clone(),
        view: request.scope.view.clone(),
        offset,
        length: None,
        sha256: None,
        status: CarveStatus::Rejected,
        reason: "invalidPng",
        validator_version: PNG_VALIDATOR_VERSION,
    };
    let limit = request
        .scope
        .end()?
        .min(offset.saturating_add(request.limits.max_object_bytes));
    reader.seek(SeekFrom::Start(offset))?;
    let mut input = PngInput {
        reader,
        at: offset,
        end: limit,
        object_limit: limit < request.scope.end()?,
        budget,
        hash: Sha256::new(),
        cancel: request.cancel,
    };
    let validation = validate_chunks(&mut input, request);
    match validation {
        Ok(()) => {
            candidate.length = Some(input.at - offset);
            candidate.sha256 = Some(format!("{:x}", input.hash.finalize()));
            candidate.status = CarveStatus::Complete;
            candidate.reason = "verifiedCrcZlibAndRaster";
        }
        Err(PngError::Invalid(reason)) => candidate.reason = reason,
        Err(PngError::Truncated) => {
            candidate.status = CarveStatus::Partial;
            candidate.reason = "truncatedPng";
        }
        Err(PngError::Core(error)) => return Err(error),
    }
    Ok(candidate)
}

enum PngError {
    Invalid(&'static str),
    Truncated,
    Core(CarveError),
}
struct PngInput<'a, R: ?Sized> {
    reader: &'a mut R,
    at: u64,
    end: u64,
    object_limit: bool,
    budget: &'a mut ValidationBudget,
    hash: Sha256,
    cancel: &'a std::sync::atomic::AtomicBool,
}
impl<R: Read + ?Sized> PngInput<'_, R> {
    fn read(&mut self, output: &mut [u8]) -> Result<(), PngError> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(PngError::Core(CarveError::Cancelled));
        }
        if output.len() as u64 > self.end - self.at {
            return Err(if self.object_limit {
                PngError::Invalid("objectLimit")
            } else {
                PngError::Truncated
            });
        }
        if output.len() as u64 > self.budget.input {
            return Err(PngError::Core(CarveError::Limit));
        }
        self.reader.read_exact(output).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                PngError::Truncated
            } else {
                PngError::Core(CarveError::Io(e))
            }
        })?;
        self.budget.input -= output.len() as u64;
        self.at += output.len() as u64;
        self.hash.update(output);
        Ok(())
    }
}

fn validate_chunks<R: Read + ?Sized>(
    input: &mut PngInput<'_, R>,
    request: &ScanRequest<'_>,
) -> Result<(), PngError> {
    let mut magic = [0u8; 8];
    input.read(&mut magic)?;
    if &magic != PNG_SIGNATURE {
        return Err(PngError::Invalid("invalidSignature"));
    }
    let mut raster = None;
    let mut idat = false;
    let mut closed_idat = false;
    let mut palette = false;
    for index in 0..request.limits.max_chunks {
        let mut header = [0u8; 8];
        input.read(&mut header)?;
        let len = u32::from_be_bytes(
            header[..4]
                .try_into()
                .map_err(|_| PngError::Invalid("invalidChunk"))?,
        );
        let kind = &header[4..8];
        if len > request.limits.max_chunk_bytes {
            return Err(PngError::Invalid("chunkLimit"));
        }
        if !kind.iter().all(u8::is_ascii_alphabetic) || !kind[2].is_ascii_uppercase() {
            return Err(PngError::Invalid("invalidChunkType"));
        }
        if (index == 0 && (kind != b"IHDR" || len != 13)) || (index != 0 && kind == b"IHDR") {
            return Err(PngError::Invalid("invalidHeaderOrder"));
        }
        if kind == b"IDAT" && closed_idat {
            return Err(PngError::Invalid("invalidIdatOrder"));
        }
        if kind == b"PLTE" {
            if palette || idat || !raster.as_ref().is_some_and(|r: &Raster| r.palette_allowed) {
                return Err(PngError::Invalid("invalidPaletteOrder"));
            }
            palette = true;
        }
        if idat && kind != b"IDAT" {
            closed_idat = true;
        }
        if kind[0].is_ascii_uppercase()
            && ![b"IHDR", b"PLTE", b"IDAT", b"IEND"]
                .iter()
                .any(|k| kind == *k)
        {
            return Err(PngError::Invalid("unsupportedCriticalChunk"));
        }
        let mut crc = crc32fast::Hasher::new();
        crc.update(kind);
        read_chunk(input, len, kind, &mut crc, &mut raster)?;
        let mut checksum = [0u8; 4];
        input.read(&mut checksum)?;
        if crc.finalize() != u32::from_be_bytes(checksum) {
            return Err(PngError::Invalid("crcMismatch"));
        }
        if kind == b"IDAT" {
            idat = true;
        }
        if kind == b"IEND" {
            return if len == 0 && idat && raster.as_ref().is_some_and(Raster::complete) {
                Ok(())
            } else {
                Err(PngError::Invalid("incompleteRaster"))
            };
        }
    }
    Err(PngError::Invalid("chunkCountLimit"))
}

fn read_chunk<R: Read + ?Sized>(
    input: &mut PngInput<'_, R>,
    len: u32,
    kind: &[u8],
    crc: &mut crc32fast::Hasher,
    raster: &mut Option<Raster>,
) -> Result<(), PngError> {
    if kind == b"IHDR" {
        let mut bytes = [0u8; 13];
        input.read(&mut bytes)?;
        crc.update(&bytes);
        *raster = Some(Raster::new(&bytes).map_err(PngError::Invalid)?);
        return Ok(());
    }
    if kind == b"PLTE" && (len == 0 || len > 768 || !len.is_multiple_of(3)) {
        return Err(PngError::Invalid("invalidPalette"));
    }
    let mut buffer = [0u8; 65536];
    let mut left = len as usize;
    while left > 0 {
        let amount = left.min(buffer.len());
        input.read(&mut buffer[..amount])?;
        crc.update(&buffer[..amount]);
        if kind == b"IDAT" {
            let result = raster
                .as_mut()
                .ok_or(PngError::Invalid("missingHeader"))?
                .consume(&buffer[..amount], &mut input.budget.output, input.cancel);
            result.map_err(|reason| {
                if reason == "cancelled" {
                    PngError::Core(CarveError::Cancelled)
                } else if reason == "validationBudget" {
                    PngError::Core(CarveError::Limit)
                } else {
                    PngError::Invalid(reason)
                }
            })?;
        }
        left -= amount;
    }
    Ok(())
}
