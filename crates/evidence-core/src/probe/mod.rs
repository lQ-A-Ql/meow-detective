use std::path::Path;

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub candidates: Vec<String>,
    pub size: u64,
    pub can_read_sectors: bool,
}

pub fn probe(path: &Path) -> ProberResult {
    if !path.exists() {
        return Err(ProbeError::NotFound(path.to_path_buf()));
    }

    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();

    let mut candidates = Vec::new();
    let mut can_read_sectors = false;

    if path.is_dir() {
        candidates.push("logical_directory".to_string());
        return Ok(ProbeResult {
            candidates,
            size,
            can_read_sectors: false,
        });
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if let Some(kind) = detect_vhd(path)? {
        candidates.push(kind.to_string());
        can_read_sectors = kind == "vhd-fixed";
        return Ok(ProbeResult {
            candidates,
            size,
            can_read_sectors,
        });
    }

    if ext == "e01" || ext == "ewf" {
        candidates.push("e01".to_string());
        can_read_sectors = true;
    }
    if matches!(ext.as_str(), "dd" | "raw" | "img" | "bin" | "001") {
        candidates.push("raw".to_string());
        can_read_sectors = true;
    }

    if candidates.is_empty() {
        let mut file = std::fs::File::open(path)?;
        let mut magic = [0u8; 8];
        use std::io::Read;
        if file.read_exact(&mut magic).is_ok()
            && (&magic[0..8] == b"EVF\x09\x0d\x0a\xff\x00" || &magic[0..3] == b"EVF")
        {
            candidates.push("e01".to_string());
            can_read_sectors = true;
        }
        // Unknown containers are deliberately not treated as raw images.
        // Callers must obtain an explicit capability before opening sectors.
    }

    Ok(ProbeResult {
        candidates,
        size,
        can_read_sectors,
    })
}

/// Returns a VHD capability label when the footer has the VHD cookie.
/// Dynamic and differencing images are surfaced explicitly so callers cannot
/// silently treat them as raw bytes.
fn detect_vhd(path: &Path) -> std::io::Result<Option<&'static str>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    if len < 512 {
        return Ok(None);
    }
    file.seek(SeekFrom::End(-512))?;
    let mut footer = [0u8; 512];
    file.read_exact(&mut footer)?;
    if &footer[..8] != b"conectix" {
        return Ok(None);
    }
    let kind = match u32::from_be_bytes(footer[60..64].try_into().expect("fixed slice")) {
        2 => "vhd-fixed",
        3 => "vhd-dynamic",
        4 => "vhd-differencing",
        _ => "vhd-unsupported",
    };
    Ok(Some(kind))
}

pub type ProberResult = Result<ProbeResult, ProbeError>;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("Path not found: {0}")]
    NotFound(std::path::PathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
