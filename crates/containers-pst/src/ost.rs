//! OST (Offline Storage Table) format support.
//!
//! OST and PST files share the same NDB/LTP format. `OstReader` delegates
//! parsing to `PstReader` while retaining an OST-specific type boundary.

use std::path::Path;

use crate::pst::PstReader;
use crate::{PstCalendar, PstContact, PstError, PstFolder, PstMessage};

/// Recognized flavor of an Outlook data file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlookFileKind {
    /// A standard PST (Personal Storage Table) file.
    Pst,
    /// An OST (Offline Storage Table) cached Exchange mailbox.
    Ost,
}

/// OST-specific properties that may differ from PST.
#[derive(Debug, Clone)]
pub struct OstProperties {
    /// Whether the OST is encrypted at the Exchange layer.
    pub encrypted: bool,
    /// The file kind detected from the path and header.
    pub file_kind: OutlookFileKind,
}

/// A read-only OST file reader.
///
/// Wraps a [`PstReader`] to reuse NDB/LTP parsing while providing OST-specific
/// type-level distinction.
pub struct OstReader {
    inner: PstReader,
    properties: OstProperties,
}

impl OstReader {
    /// Open an OST or PST file from the given path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PstError> {
        let path_ref = path.as_ref();
        let inner = PstReader::open(path_ref)?;
        let file_kind = detect_file_kind(path_ref, &inner);

        Ok(Self {
            inner,
            properties: OstProperties {
                encrypted: false,
                file_kind,
            },
        })
    }

    /// Return the detected file kind.
    pub fn file_kind(&self) -> OutlookFileKind {
        self.properties.file_kind
    }

    /// Return whether this is a Unicode file.
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

    /// Extract all email messages.
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

fn detect_file_kind(path: &Path, _reader: &PstReader) -> OutlookFileKind {
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ost"))
    {
        return OutlookFileKind::Ost;
    }

    OutlookFileKind::Pst
}

#[cfg(test)]
#[path = "../tests/unit/ost.rs"]
mod tests;
