use std::path::Path;

#[derive(Debug, Clone)]
pub struct ProbeResult {
    pub candidates: Vec<String>,
    pub size: u64,
}

pub fn probe(path: &Path) -> ProberResult {
    if !path.exists() {
        return Err(ProbeError::NotFound(path.to_path_buf()));
    }

    let metadata = std::fs::metadata(path)?;
    let size = metadata.len();

    let mut candidates = Vec::new();

    if path.is_dir() {
        candidates.push("logical_directory".to_string());
        return Ok(ProbeResult { candidates, size });
    }

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "e01" | "ewf" => candidates.push("e01".to_string()),
        "dd" | "raw" | "img" | "bin" | "001" => candidates.push("raw".to_string()),
        _ => {}
    }

    if candidates.is_empty() {
        let mut file = std::fs::File::open(path)?;
        let mut magic = [0u8; 8];
        use std::io::Read;
        if file.read_exact(&mut magic).is_ok()
            && (&magic[0..8] == b"EVF\x09\x0d\x0a\xff\x00" || &magic[0..3] == b"EVF")
        {
            candidates.push("e01".to_string());
        }
        if candidates.is_empty() {
            candidates.push("raw".to_string());
        }
    }

    Ok(ProbeResult { candidates, size })
}

pub type ProberResult = Result<ProbeResult, ProbeError>;

#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    #[error("Path not found: {0}")]
    NotFound(std::path::PathBuf),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
