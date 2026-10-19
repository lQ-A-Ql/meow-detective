//! Checksums for synthetic EWF fixtures created by tests, never acquired evidence.
use std::{
    fs::OpenOptions,
    io::{self, Read, Seek, SeekFrom, Write},
    path::Path,
};

/// Finish descriptors and offset checksums after a synthetic fixture is written.
pub fn finalize_synthetic_e01(path: &Path) -> io::Result<()> {
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let mut offset = 13u64;
    for _ in 0..100_000 {
        file.seek(SeekFrom::Start(offset))?;
        let mut descriptor = [0; 76];
        file.read_exact(&mut descriptor)?;
        let kind = String::from_utf8_lossy(&descriptor[..16])
            .trim_end_matches('\0')
            .to_string();
        let next = u64::from_le_bytes(descriptor[16..24].try_into().unwrap_or([0; 8]));
        if kind == "table" || kind == "table2" {
            finish_table(&mut file, offset + 76)?;
        }
        let checksum = adler(&descriptor[..72]);
        file.seek(SeekFrom::Start(offset + 72))?;
        file.write_all(&checksum.to_le_bytes())?;
        if kind == "done" || next == 0 {
            return file.flush();
        }
        if next <= offset {
            break;
        }
        offset = next;
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "synthetic EWF section chain is invalid",
    ))
}

fn finish_table(file: &mut std::fs::File, start: u64) -> io::Result<()> {
    file.seek(SeekFrom::Start(start))?;
    let mut header = [0; 24];
    file.read_exact(&mut header)?;
    let count = u32::from_le_bytes(header[..4].try_into().unwrap_or([0; 4])) as usize;
    if count > 16 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "synthetic EWF table exceeds fixture limit",
        ));
    }
    let checksum = adler(&header[..20]);
    file.seek(SeekFrom::Start(start + 20))?;
    file.write_all(&checksum.to_le_bytes())?;
    let mut offsets = vec![0; count * 4];
    file.read_exact(&mut offsets)?;
    file.write_all(&adler(&offsets).to_le_bytes())?;
    Ok(())
}

fn adler(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for byte in bytes {
        a = (a + u32::from(*byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
