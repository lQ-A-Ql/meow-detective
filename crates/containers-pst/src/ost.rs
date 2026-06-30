//! OST (Offline Storage Table) format support.
//!
//! OST files are used by Microsoft Outlook in cached Exchange mode to store
//! a local copy of mailbox data for offline access.
//!
//! ## Relationship to PST
//!
//! OST and PST files share the same underlying NDB/LTP format:
//! - Both use the "!BDN" magic bytes at offset 0.
//! - Both use 512-byte pages and the same Node/Block BTree structures.
//! - Both use the same property store (Heap-on-Node and BTree-on-Heap).
//!
//! The key differences that distinguish an OST from a PST:
//!
//! | Aspect        | PST                     | OST                        |
//! |---------------|-------------------------|----------------------------|
//! | Purpose       | Personal storage        | Cached Exchange (offline)  |
//! | Ownership     | User-managed archives   | Synced with Exchange server|
//! | Header flag   | dwReserved = 0          | dwReserved may have flags  |
//! | Client sig    | Standard                | May differ for Exchange    |
//!
//! For the MVP, `OstReader` delegates all parsing to `PstReader` while
//! marking the file as an offline-cache with its own type wrapper. This
//! allows callers to distinguish OST from PST at the type level without
//! duplicating the NDB/LTP parsing logic.
//!
//! ## Future enhancements
//!
//! - Detect Exchange-specific properties (PR_OST_ENCRYPTION, etc.)
//! - Handle OST-specific synchronization metadata
//! - Respect OST password/encryption if present

use std::path::Path;

use crate::pst::PstReader;
use crate::{PstCalendar, PstContact, PstError, PstFolder, PstMessage};

/// Recognized flavor of an Outlook data file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlookFileKind {
    /// A standard PST (Personal Storage Table) file.
    Pst,
    /// An OST (Offline Storage Table) file — cached Exchange mailbox.
    Ost,
}

/// An OST-specific property that may differ from PST.
#[derive(Debug, Clone)]
pub struct OstProperties {
    /// Whether the OST is encrypted (Exchange-level).
    pub encrypted: bool,
    /// The file kind detected from the header.
    pub file_kind: OutlookFileKind,
}

// ─── OstReader ─────────────────────────────────────────────────────────────

/// A read-only OST file reader.
///
/// Wraps a [`PstReader`] to reuse NDB/LTP parsing while providing OST-specific
/// type-level distinction. OST and PST share the same on-disk format; the
/// reader detects the file kind (PST vs OST) from header flags.
///
/// # Examples
///
/// ```ignore
/// let reader = OstReader::open("mailbox.ost").unwrap();
/// assert_eq!(reader.file_kind(), OutlookFileKind::Ost);
/// let messages = reader.read_messages().unwrap();
/// ```
pub struct OstReader {
    /// Delegate PST reader for all NDB/LTP parsing.
    inner: PstReader,
    /// OST-specific properties and file kind.
    properties: OstProperties,
}

impl OstReader {
    /// Open an OST (or PST) file from the given path.
    ///
    /// Detects the file kind from the header and delegates parsing to
    /// the underlying PST reader.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PstError> {
        let path_ref = path.as_ref();
        let inner = PstReader::open(path_ref)?;
        let file_kind = detect_file_kind(path_ref, &inner);

        let encrypted = false; // MVP: encryption detection not yet implemented

        Ok(Self {
            inner,
            properties: OstProperties {
                encrypted,
                file_kind,
            },
        })
    }

    /// Return the detected file kind (PST or OST).
    pub fn file_kind(&self) -> OutlookFileKind {
        self.properties.file_kind
    }

    /// Return whether this is a Unicode (64-bit) file.
    pub fn is_unicode(&self) -> bool {
        self.inner.is_unicode()
    }

    /// Return the file size declared in the header.
    pub fn file_size(&self) -> u64 {
        self.inner.file_size()
    }

    /// Return OST-specific properties.
    pub fn ost_properties(&self) -> &OstProperties {
        &self.properties
    }

    // ── Delegated extraction methods ─────────────────────────────────────

    /// Extract all email messages from the file.
    pub fn read_messages(&self) -> Result<Vec<PstMessage>, PstError> {
        self.inner.read_messages()
    }

    /// Extract the folder hierarchy.
    pub fn read_folders(&self) -> Result<Vec<PstFolder>, PstError> {
        self.inner.read_folders()
    }

    /// Extract calendar entries.
    pub fn read_calendar(&self) -> Result<Vec<PstCalendar>, PstError> {
        self.inner.read_calendar()
    }

    /// Extract contact entries.
    pub fn read_contacts(&self) -> Result<Vec<PstContact>, PstError> {
        self.inner.read_contacts()
    }
}

// ─── File kind detection ──────────────────────────────────────────────────

/// Detect whether the opened file is a PST or OST.
///
/// Currently this is a heuristic based on well-known patterns. A more precise
/// detection would examine the `dwReserved` field in the header ROOT structure
/// or look for Exchange-specific synchronization NIDs.
fn detect_file_kind(path: &Path, _reader: &PstReader) -> OutlookFileKind {
    // Primary signal: file extension. OST and PST share the same NDB/LTP
    // format, so the extension is the most reliable caller-provided hint.
    if let Some(ext) = path.extension() {
        if ext.eq_ignore_ascii_case("ost") {
            return OutlookFileKind::Ost;
        }
    }

    // Future: fall back to examining `bClientSig` in header, `dwReserved` in
    // ROOT, or presence of known Exchange NIDs (e.g., 0x35 for special folders).
    OutlookFileKind::Pst
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal synthetic OST file in memory.
    ///
    /// Uses the same NDB/LTP structure as a PST. For the MVP, the binary
    /// layout is identical to a PST — OST detection happens via caller context
    /// or file extension, not via binary differentiation.
    fn build_synthetic_ost() -> Vec<u8> {
        // Reuse the PST synthetic builder to produce a valid NDB/LTP file.
        // In practice, OST and PST share the same binary layout; the
        // distinction is in the header's dwReserved or bClientSig fields.
        let data = crate::pst::build_synthetic_pst();
        // If we had an OST-specific flag, we would set it here.
        // For example: data[14] = 0x02 (bPlatformCreate OST variant).
        data
    }

    #[test]
    fn ost_opens_valid_ndb_file() {
        let data = build_synthetic_ost();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ost");
        std::fs::write(&path, &data).unwrap();

        let reader = OstReader::open(&path).unwrap();
        assert!(reader.is_unicode());
        assert!(reader.file_size() > 0);
    }

    #[test]
    fn ost_file_kind_detected_by_extension() {
        let data = build_synthetic_ost();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ost");
        std::fs::write(&path, &data).unwrap();

        let reader = OstReader::open(&path).unwrap();
        assert_eq!(reader.file_kind(), OutlookFileKind::Ost);
    }

    #[test]
    fn ost_file_kind_defaults_to_pst_for_pst_extension() {
        let data = build_synthetic_ost();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.pst");
        std::fs::write(&path, &data).unwrap();

        let reader = OstReader::open(&path).unwrap();
        assert_eq!(reader.file_kind(), OutlookFileKind::Pst);
    }

    #[test]
    fn ost_reader_delegates_to_pst() {
        let data = build_synthetic_ost();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ost");
        std::fs::write(&path, &data).unwrap();

        let reader = OstReader::open(&path).unwrap();

        // Delegated methods should work.
        let folders = reader.read_folders().unwrap();
        // At minimum, we should get back some folder entries from the NBT cache.
        // The synthetic fixture has root folder entries.
        assert!(!folders.is_empty(), "Should have at least one folder");
    }

    #[test]
    fn ost_ost_properties_accessible() {
        let data = build_synthetic_ost();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.ost");
        std::fs::write(&path, &data).unwrap();

        let reader = OstReader::open(&path).unwrap();
        let props = reader.ost_properties();
        assert!(!props.encrypted);
    }

    #[test]
    fn ost_rejects_invalid_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.ost");
        std::fs::write(&path, b"not an ost file").unwrap();

        let result = OstReader::open(&path);
        assert!(result.is_err());
    }
}
