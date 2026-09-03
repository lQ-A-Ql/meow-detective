use crate::filesystem::{join_child_path, FileSystemReader, FsNode, ReadSeek};
use crate::image::raw_reader::RawImageReader;
use crate::EvidenceReader;
use std::collections::{HashMap, HashSet};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};

const BLOCK_SIZE: u64 = 2048;
const MAX_VOLUME_DESCRIPTORS: usize = 256;
const MAX_DIRECTORY_BYTES: u32 = 64 * 1024 * 1024;
const MAX_DIRECTORY_DEPTH: usize = 64;
const MAX_DIRECTORY_ENTRIES: usize = 1_000_000;

#[derive(Debug, Clone)]
struct IsoEntry {
    name: String,
    path: String,
    is_dir: bool,
    size: u64,
    extent: u64,
    hidden: bool,
}

#[derive(Debug, Clone, Copy)]
struct IsoVolumeContext {
    joliet: bool,
    byte_len: u64,
}

/// Read-only ISO9660 reader with Joliet name support.
///
/// Rock Ridge and UDF extensions are deliberately outside this adapter. The
/// reader accepts a Primary Volume Descriptor and prefers a valid Joliet
/// Supplementary Volume Descriptor when present. Directory records are
/// bounded and validated before they are indexed.
pub struct Iso9660Reader {
    reader: Arc<Mutex<Box<dyn EvidenceReader>>>,
    data_source_name: String,
    directories: HashMap<String, Vec<IsoEntry>>,
    files: HashMap<String, IsoEntry>,
}

impl std::fmt::Debug for Iso9660Reader {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Iso9660Reader")
            .field("data_source_name", &self.data_source_name)
            .field("directory_count", &self.directories.len())
            .field("file_count", &self.files.len())
            .finish_non_exhaustive()
    }
}

impl Iso9660Reader {
    pub fn open(path: &Path) -> io::Result<Self> {
        let reader = RawImageReader::open(path)?;
        Self::from_reader(
            Box::new(reader),
            path.file_name().and_then(|name| name.to_str()),
        )
    }

    pub fn from_reader(
        mut reader: Box<dyn EvidenceReader>,
        data_source_name: Option<&str>,
    ) -> io::Result<Self> {
        let (primary, joliet) = read_volume_descriptors(reader.as_mut())?;
        let descriptor = joliet.as_ref().unwrap_or(&primary);
        let volume_blocks = read_both_endian_u32(descriptor, 80)?;
        let volume_bytes = u64::from(volume_blocks)
            .checked_mul(BLOCK_SIZE)
            .ok_or_else(|| invalid_data("ISO9660 volume size overflows u64"))?;
        if volume_blocks == 0 {
            return Err(invalid_data("ISO9660 declared volume is empty"));
        }
        if volume_bytes > reader.info().size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "ISO9660 declared volume exceeds the evidence reader",
            ));
        }
        let root = parse_root_record(descriptor)?;
        validate_extent(root.0, u64::from(root.1), volume_bytes)?;
        let volume = IsoVolumeContext {
            joliet: joliet.is_some(),
            byte_len: volume_bytes,
        };
        let mut output = Self {
            reader: Arc::new(Mutex::new(reader)),
            data_source_name: data_source_name.unwrap_or("ISO image").to_string(),
            directories: HashMap::new(),
            files: HashMap::new(),
        };
        let mut visited = HashSet::new();
        let reader = Arc::clone(&output.reader);
        let mut reader = reader
            .lock()
            .map_err(|_| io::Error::other("ISO9660 reader lock is poisoned"))?;
        output.parse_directory(reader.as_mut(), "", root, volume, 0, &mut visited)?;
        drop(reader);
        Ok(output)
    }

    fn parse_directory(
        &mut self,
        reader: &mut dyn EvidenceReader,
        parent: &str,
        location: (u64, u32),
        volume: IsoVolumeContext,
        depth: usize,
        visited: &mut HashSet<u64>,
    ) -> io::Result<()> {
        let (extent, size) = location;
        if depth > MAX_DIRECTORY_DEPTH || size > MAX_DIRECTORY_BYTES {
            return Err(invalid_data("ISO9660 directory exceeds safety limits"));
        }
        if !visited.insert(extent) {
            return Ok(());
        }
        let byte_offset = extent
            .checked_mul(BLOCK_SIZE)
            .ok_or_else(|| invalid_data("ISO9660 directory offset overflows u64"))?;
        reader.seek(SeekFrom::Start(byte_offset))?;
        let mut bytes = vec![0u8; size as usize];
        reader.read_exact(&mut bytes)?;
        let mut children = Vec::new();
        let mut position = 0usize;
        while position < bytes.len() {
            let length = bytes[position] as usize;
            if length == 0 {
                position = ((position / BLOCK_SIZE as usize) + 1) * BLOCK_SIZE as usize;
                continue;
            }
            if length < 34 || position + length > bytes.len() {
                return Err(invalid_data("ISO9660 directory record is truncated"));
            }
            if position % BLOCK_SIZE as usize + length > BLOCK_SIZE as usize {
                return Err(invalid_data(
                    "ISO9660 directory record crosses a block boundary",
                ));
            }
            let record = &bytes[position..position + length];
            if record[1] != 0 || record[26] != 0 || record[27] != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "ISO9660 extended attributes and interleaved extents are unsupported",
                ));
            }
            if record[25] & 0x80 != 0 {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "ISO9660 multi-extent files are unsupported",
                ));
            }
            let name_len = record[32] as usize;
            if 33 + name_len > record.len() {
                return Err(invalid_data("ISO9660 directory name exceeds record"));
            }
            let raw_name = &record[33..33 + name_len];
            if raw_name != [0] && raw_name != [1] {
                let name = decode_name(raw_name, volume.joliet)?;
                if !name.is_empty() && name != "." && name != ".." {
                    let path = join_child_path(parent, &name);
                    let child = IsoEntry {
                        name,
                        path: path.clone(),
                        is_dir: record[25] & 0x02 != 0,
                        size: read_both_endian_u32(record, 10)? as u64,
                        extent: read_both_endian_u32(record, 2)? as u64,
                        hidden: record[25] & 0x01 != 0,
                    };
                    validate_extent(child.extent, child.size, volume.byte_len)?;
                    if self.files.len() + self.directories.len() + children.len()
                        >= MAX_DIRECTORY_ENTRIES
                    {
                        return Err(invalid_data("ISO9660 contains too many directory entries"));
                    }
                    children.push(child.clone());
                    if child.is_dir {
                        self.parse_directory(
                            reader,
                            &path,
                            (child.extent, child.size as u32),
                            volume,
                            depth + 1,
                            visited,
                        )?;
                    } else {
                        self.files.insert(path, child);
                    }
                }
            }
            position += length;
        }
        children.sort_by(|a, b| a.is_dir.cmp(&b.is_dir).reverse().then(a.name.cmp(&b.name)));
        self.directories.insert(parent.to_string(), children);
        Ok(())
    }
}

impl FileSystemReader for Iso9660Reader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(FsNode {
            name: self.data_source_name.clone(),
            path: String::new(),
            is_dir: true,
            size: 0,
            hidden: false,
            system: false,
            read_only: true,
            encrypted: false,
            archive: false,
            unix_mode: None,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
        })
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let key = normalize_path(path);
        let children = self.directories.get(&key).ok_or_else(|| {
            io::Error::new(io::ErrorKind::NotFound, "ISO9660 directory not found")
        })?;
        Ok(children
            .iter()
            .map(|entry| FsNode {
                name: entry.name.clone(),
                path: entry.path.clone(),
                is_dir: entry.is_dir,
                size: entry.size,
                hidden: entry.hidden,
                system: false,
                read_only: true,
                encrypted: false,
                archive: false,
                unix_mode: None,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
            })
            .collect())
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        Ok(self.open_iso_file(path)?)
    }

    fn open_file_seekable(&self, path: &str) -> io::Result<Box<dyn ReadSeek>> {
        Ok(self.open_iso_file(path)?)
    }

    fn data_source_name(&self) -> &str {
        &self.data_source_name
    }
}

impl Iso9660Reader {
    fn open_iso_file(&self, path: &str) -> io::Result<Box<IsoFileReader>> {
        let key = normalize_path(path);
        let entry = self
            .files
            .get(&key)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "ISO9660 file not found"))?;
        let offset = entry
            .extent
            .checked_mul(BLOCK_SIZE)
            .ok_or_else(|| invalid_data("ISO9660 file offset overflows u64"))?;
        Ok(Box::new(IsoFileReader {
            reader: Arc::clone(&self.reader),
            offset,
            length: entry.size,
            position: 0,
        }))
    }
}

struct IsoFileReader {
    reader: Arc<Mutex<Box<dyn EvidenceReader>>>,
    offset: u64,
    length: u64,
    position: u64,
}

impl Read for IsoFileReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.length || buffer.is_empty() {
            return Ok(0);
        }
        let amount = (self.length - self.position).min(buffer.len() as u64) as usize;
        let mut reader = self
            .reader
            .lock()
            .map_err(|_| io::Error::other("ISO9660 reader lock is poisoned"))?;
        reader.seek(SeekFrom::Start(self.offset + self.position))?;
        let read = reader.read(&mut buffer[..amount])?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "ISO9660 file extent ended before its declared length",
            ));
        }
        self.position = self.position.saturating_add(read as u64);
        Ok(read)
    }
}

impl Seek for IsoFileReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let next = match position {
            SeekFrom::Start(offset) => i128::from(offset),
            SeekFrom::Current(offset) => i128::from(self.position) + i128::from(offset),
            SeekFrom::End(offset) => i128::from(self.length) + i128::from(offset),
        };
        if next < 0 || next > i128::from(u64::MAX) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek position is outside the ISO9660 file",
            ));
        }
        self.position = next as u64;
        Ok(self.position)
    }
}

fn read_volume_descriptors(
    reader: &mut dyn EvidenceReader,
) -> io::Result<(Vec<u8>, Option<Vec<u8>>)> {
    let mut primary = None;
    let mut joliet = None;
    for index in 0..MAX_VOLUME_DESCRIPTORS {
        let offset = (16 + index) as u64 * BLOCK_SIZE;
        reader.seek(SeekFrom::Start(offset))?;
        let mut descriptor = vec![0u8; BLOCK_SIZE as usize];
        if let Err(error) = reader.read_exact(&mut descriptor) {
            if error.kind() == io::ErrorKind::UnexpectedEof && primary.is_none() {
                return Err(invalid_data("ISO9660 Primary Volume Descriptor not found"));
            }
            if error.kind() == io::ErrorKind::UnexpectedEof {
                break;
            }
            return Err(error);
        }
        if &descriptor[1..6] != b"CD001" || descriptor[6] != 1 {
            continue;
        }
        if matches!(descriptor[0], 1 | 2) {
            let block_size = u16::from_le_bytes([descriptor[128], descriptor[129]]);
            let block_size_be = u16::from_be_bytes([descriptor[130], descriptor[131]]);
            if block_size != BLOCK_SIZE as u16 || block_size_be != block_size {
                return Err(invalid_data("ISO9660 logical block size is not 2048 bytes"));
            }
        }
        match descriptor[0] {
            1 if primary.is_none() => primary = Some(descriptor),
            2 if is_joliet_descriptor(&descriptor) && joliet.is_none() => joliet = Some(descriptor),
            255 => break,
            _ => {}
        }
    }
    let primary =
        primary.ok_or_else(|| invalid_data("ISO9660 Primary Volume Descriptor not found"))?;
    Ok((primary, joliet))
}

fn parse_root_record(descriptor: &[u8]) -> io::Result<(u64, u32)> {
    let root = descriptor
        .get(156..190)
        .ok_or_else(|| invalid_data("ISO9660 root directory record is missing"))?;
    if root[0] < 34
        || root[1] != 0
        || root[25] & 0x02 == 0
        || root[25] & 0x80 != 0
        || root[26] != 0
        || root[27] != 0
    {
        return Err(invalid_data("ISO9660 root directory record is invalid"));
    }
    let extent = read_both_endian_u32(root, 2)? as u64;
    let size = read_both_endian_u32(root, 10)?;
    if extent == 0 || size == 0 || size > MAX_DIRECTORY_BYTES {
        return Err(invalid_data("ISO9660 root directory record is invalid"));
    }
    Ok((extent, size))
}

fn read_both_endian_u32(bytes: &[u8], offset: usize) -> io::Result<u32> {
    let fields = bytes
        .get(offset..offset + 8)
        .ok_or_else(|| invalid_data("ISO9660 both-endian field is truncated"))?;
    let little = u32::from_le_bytes(fields[..4].try_into().unwrap_or([0; 4]));
    let big = u32::from_be_bytes(fields[4..].try_into().unwrap_or([0; 4]));
    if little != big {
        return Err(invalid_data("ISO9660 both-endian field values disagree"));
    }
    Ok(little)
}

fn validate_extent(extent: u64, size: u64, volume_bytes: u64) -> io::Result<()> {
    let start = extent
        .checked_mul(BLOCK_SIZE)
        .ok_or_else(|| invalid_data("ISO9660 extent offset overflows u64"))?;
    let end = start
        .checked_add(size)
        .ok_or_else(|| invalid_data("ISO9660 extent length overflows u64"))?;
    if end > volume_bytes {
        return Err(invalid_data(
            "ISO9660 extent exceeds the declared volume boundary",
        ));
    }
    Ok(())
}

fn is_joliet_descriptor(descriptor: &[u8]) -> bool {
    descriptor
        .get(88..91)
        .is_some_and(|escape| matches!(escape, b"%/@" | b"%/C" | b"%/E"))
}

fn decode_name(raw: &[u8], joliet: bool) -> io::Result<String> {
    let mut name = if joliet {
        if !raw.len().is_multiple_of(2) {
            return Err(invalid_data("Joliet file name has an odd byte length"));
        }
        let units = raw
            .chunks_exact(2)
            .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
            .collect::<Vec<_>>();
        String::from_utf16(&units)
            .map_err(|_| invalid_data("Joliet file name is invalid UTF-16"))?
    } else {
        String::from_utf8_lossy(raw).into_owned()
    };
    if let Some((base, version)) = name.rsplit_once(';') {
        if version.chars().all(|ch| ch.is_ascii_digit()) {
            name = base.to_string();
        }
    }
    Ok(name.trim_end_matches('.').replace(['/', '\\'], "_"))
}

fn normalize_path(path: &str) -> String {
    path.trim_matches(['/', '\\'])
        .split(['/', '\\'])
        .filter(|part| !part.is_empty() && *part != "." && *part != "..")
        .collect::<Vec<_>>()
        .join("/")
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
#[path = "../../tests/unit/image/iso9660.rs"]
mod tests;
