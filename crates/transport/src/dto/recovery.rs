use serde::{Deserialize, Serialize};

/// A file recovered from filesystem journal/log analysis.
///
/// This DTO represents a file reconstructed from journal entries
/// (ext4 JBD2 or XFS log) after deletion.  It captures the recovered
/// metadata and a confidence score.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedFileRecoveryDto {
    /// Best-guess original path (may be an $OrphanInode synthetic path).
    pub original_path: String,
    /// Filesystem-specific inode number (u32 for ext4, u64 for XFS —
    /// stored as string for transport portability).
    pub inode: String,
    /// Declared file size in bytes according to the recovered inode.
    pub declared_size: u64,
    /// Number of data blocks recovered.
    pub block_count: u64,
    /// How the file was recovered (e.g. "journal_descriptor",
    /// "xlog_inode_item_format_2", "dirent_hint").
    pub recovery_method: String,
    /// Confidence score 0.0–1.0: higher means more complete recovery.
    pub confidence: f64,
    /// Data source identifier (e.g. "ext4", "xfs").
    pub filesystem_type: String,
    /// Number of raw bytes recovered (sum of block sizes).
    pub recovered_bytes: u64,
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "../../tests/unit/dto/recovery.rs"]
mod tests;
