use std::io::{self, Read, Seek, SeekFrom};
use std::path::PathBuf;

use domain::{CaseId, DataSourceId, FileEntryId};
use evidence_core::{EvidenceReader, EvidenceReaderFactory, ReaderCapabilities, ReaderInfo};
use persistence_source::repositories::file_repo::FileRepo;
use persistence_sqlite::repositories::nested_evidence_repo::{
    NestedEvidenceRepo, NestedRangeLocator,
};

use crate::digest_service::{identity, DigestTarget};
use crate::file_service::{read_file_bytes_for_case, SourceReadContext};

/// A fixed VHD stored as a file in a registered parent source.
pub struct NestedRangeFactory {
    pub case_root: PathBuf,
    pub case_id: CaseId,
    pub parent_id: DataSourceId,
    pub file_id: FileEntryId,
    pub revision: String,
    pub length: u64,
}

impl NestedRangeFactory {
    pub fn for_source(
        conn: &rusqlite::Connection,
        case_root: &std::path::Path,
        case_id: &CaseId,
        source_id: &DataSourceId,
    ) -> io::Result<Self> {
        let locator = NestedEvidenceRepo::new(conn)
            .find_range(source_id)
            .map_err(io::Error::other)?
            .ok_or_else(|| io::Error::other(super::SourceInvalidated))?;
        Ok(Self::from_locator(case_root, case_id, &locator))
    }

    pub fn from_locator(
        case_root: &std::path::Path,
        case_id: &CaseId,
        locator: &NestedRangeLocator,
    ) -> Self {
        Self {
            case_root: case_root.to_path_buf(),
            case_id: case_id.clone(),
            parent_id: locator.parent_id.clone(),
            file_id: locator.file_id.clone(),
            revision: locator.revision.clone(),
            length: locator.length,
        }
    }

    fn open_case(&self) -> io::Result<rusqlite::Connection> {
        let conn = rusqlite::Connection::open_with_flags(
            self.case_root.join("app.db"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(io::Error::other)?;
        let repo = NestedEvidenceRepo::new(&conn);
        let mut parent = self.parent_id.clone();
        let mut file = self.file_id.clone();
        let mut revision = self.revision.clone();
        for _ in 0..4 {
            let file_id = crate::source_db::encode_source_scoped_id(&parent, &file.0);
            let current = identity::snapshot(
                &conn,
                &self.case_root,
                &self.case_id,
                &DigestTarget {
                    data_source_id: Some(&parent.0),
                    file_id: Some(&file_id),
                    partition_index: None,
                },
            )
            .map_err(io::Error::other)?;
            if current.revision != revision {
                return Err(io::Error::other(super::SourceInvalidated));
            }
            let Some(locator) = repo.find_range(&parent).map_err(io::Error::other)? else {
                return Ok(conn);
            };
            parent = locator.parent_id;
            file = locator.file_id;
            revision = locator.revision;
        }
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "nested evidence exceeds four levels",
        ))
    }
}

impl EvidenceReaderFactory for NestedRangeFactory {
    fn open_reader(&self) -> io::Result<Box<dyn EvidenceReader>> {
        let case_conn = self.open_case()?;
        let parent = crate::source_db::open_ready_source_read_only_by_id(
            &case_conn,
            &self.case_root,
            &self.case_id,
            &self.parent_id,
        )
        .map_err(io::Error::other)?;
        let physical_length = self.length.checked_add(512).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "nested VHD length overflow")
        })?;
        let entry = FileRepo::new(&parent.connection)
            .find_by_id(&self.file_id)
            .map_err(io::Error::other)?
            .filter(|entry| {
                entry.is_file()
                    && entry.data_source_id == self.parent_id
                    && entry.size == Some(physical_length)
            })
            .ok_or_else(|| io::Error::other(super::SourceInvalidated))?;
        let reader = ParentFileReader {
            source_conn: parent.connection,
            case_conn,
            case_root: self.case_root.clone(),
            case_id: self.case_id.clone(),
            parent_id: self.parent_id.clone(),
            file_id: entry.id,
            info: ReaderInfo {
                path: PathBuf::from(entry.path),
                size: physical_length,
                kind: "nested-fixed-vhd".into(),
            },
            position: 0,
        };
        evidence_core::reader::open_fixed_vhd_payload(Box::new(reader))
            .map(|reader| Box::new(reader) as Box<dyn EvidenceReader>)
    }

    fn logical_size(&self) -> u64 {
        self.length
    }

    fn preferred_read_granularity(&self) -> usize {
        512
    }

    fn capabilities(&self) -> ReaderCapabilities {
        ReaderCapabilities::default()
    }

    fn validate_snapshot(&self) -> io::Result<()> {
        self.open_case().map(drop)
    }

    fn metadata_identity(&self) -> Option<&str> {
        Some(&self.revision)
    }
}

struct ParentFileReader {
    source_conn: rusqlite::Connection,
    case_conn: rusqlite::Connection,
    case_root: PathBuf,
    case_id: CaseId,
    parent_id: DataSourceId,
    file_id: FileEntryId,
    info: ReaderInfo,
    position: u64,
}

impl Read for ParentFileReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let length = (buf.len() as u64)
            .min(self.info.size.saturating_sub(self.position))
            .min(1024 * 1024) as u32;
        if length == 0 {
            return Ok(0);
        }
        let mut context = SourceReadContext::new(
            &self.source_conn,
            &self.case_conn,
            &self.case_root,
            &self.case_id,
            &self.parent_id,
        );
        let bytes = read_file_bytes_for_case(&mut context, &self.file_id, self.position, length)
            .map_err(io::Error::other)?;
        if bytes.is_empty() || bytes.len() > length as usize {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "nested VHD range is incomplete",
            ));
        }
        buf[..bytes.len()].copy_from_slice(&bytes);
        self.position += bytes.len() as u64;
        Ok(bytes.len())
    }
}

impl Seek for ParentFileReader {
    fn seek(&mut self, origin: SeekFrom) -> io::Result<u64> {
        let next = match origin {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::Current(offset) => self.position.checked_add_signed(offset),
            SeekFrom::End(offset) => self.info.size.checked_add_signed(offset),
        };
        self.position = next.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "nested VHD seek outside address space",
            )
        })?;
        Ok(self.position)
    }
}

impl EvidenceReader for ParentFileReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}
