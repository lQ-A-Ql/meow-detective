//! Bounded section inventory; offset arrays and sector payloads stay on disk.
use crate::{checksum::adler32, open_limits::OpenBudget, SECTION_DESCRIPTOR_SIZE};
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) struct SectionSpan {
    pub kind: String,
    pub segment: usize,
    pub start: u64,
    pub next: u64,
    pub size: u64,
    pub checksum_valid: bool,
}

pub(crate) struct SegmentSections {
    pub file_len: u64,
    pub modified: std::time::SystemTime,
    pub sections: Vec<SectionSpan>,
}

pub(crate) fn cancelled(cancel: Option<&AtomicBool>) -> io::Result<()> {
    if cancel.is_some_and(|token| token.load(Ordering::Relaxed)) {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "E01 planning cancelled",
        ));
    }
    Ok(())
}

pub(crate) fn inventory(
    files: &mut [File],
    cancel: Option<&AtomicBool>,
) -> io::Result<Vec<SegmentSections>> {
    let mut budget = OpenBudget::default();
    files
        .iter_mut()
        .enumerate()
        .map(|(index, file)| {
            cancelled(cancel)?;
            let metadata = file.metadata()?;
            file.seek(SeekFrom::Start(0))?;
            let mut header = [0; 13];
            file.read_exact(&mut header)?;
            if &header[..3] != b"EVF" {
                return Err(invalid("E01 segment has an invalid EWF signature"));
            }
            let mut sections = Vec::new();
            let mut offset = 13;
            loop {
                cancelled(cancel)?;
                budget.section()?;
                let section = descriptor(file, metadata.len(), index, offset)?;
                let done = section.kind == "done" || section.next == 0;
                let next = section.next;
                if !done
                    && (next <= offset
                        || next.checked_add(76).is_none_or(|end| end > metadata.len()))
                {
                    return Err(invalid("E01 section chain has an invalid next offset"));
                }
                sections.push(section);
                if done {
                    break;
                }
                offset = next;
            }
            Ok(SegmentSections {
                file_len: metadata.len(),
                modified: metadata.modified()?,
                sections,
            })
        })
        .collect()
}

fn descriptor(file: &mut File, length: u64, segment: usize, start: u64) -> io::Result<SectionSpan> {
    if start.checked_add(76).is_none_or(|end| end > length) {
        return Err(invalid("E01 section descriptor is truncated"));
    }
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = [0; SECTION_DESCRIPTOR_SIZE as usize];
    file.read_exact(&mut bytes)?;
    Ok(SectionSpan {
        kind: String::from_utf8_lossy(&bytes[..16])
            .trim_end_matches('\0')
            .to_string(),
        segment,
        start,
        next: u64::from_le_bytes(bytes[16..24].try_into().unwrap_or([0; 8])),
        size: u64::from_le_bytes(bytes[24..32].try_into().unwrap_or([0; 8])),
        checksum_valid: adler32(&bytes[..72])
            == u32::from_le_bytes(bytes[72..76].try_into().unwrap_or([0; 4])),
    })
}

impl SectionSpan {
    pub fn content_range(&self, file_len: u64) -> io::Result<(u64, u64)> {
        let length = self
            .size
            .checked_sub(76)
            .ok_or_else(|| invalid("E01 section has invalid length"))?;
        let start = self
            .start
            .checked_add(76)
            .ok_or_else(|| invalid("E01 section offset overflow"))?;
        if start.checked_add(length).is_none_or(|end| end > file_len) {
            return Err(invalid("E01 section extends beyond its segment"));
        }
        Ok((start, length))
    }
    pub fn read_content(&self, file: &mut File, file_len: u64) -> io::Result<Vec<u8>> {
        let (start, length) = self.content_range(file_len)?;
        if length > 64 * 1024 * 1024 {
            return Err(crate::open_limits::limit());
        }
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = vec![0; length as usize];
        file.read_exact(&mut bytes)?;
        Ok(bytes)
    }
}

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
