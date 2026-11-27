use std::{cell::RefCell, collections::HashMap, io, path::Path, sync::Arc};

use domain::{DataSourceId, FileEntryId};
use evidence_core::EvidenceReader;
use serde_json::Value;

use super::{
    viewer::{self, PreviewDescriptor, PreviewPartitionCandidate, PreviewReadContext},
    FileServiceError,
};

type OpenReader = dyn Fn(&Path, &str) -> io::Result<Box<dyn EvidenceReader>> + Send + Sync;

#[derive(Clone)]
pub struct BoundEvidenceReaderResolver(Arc<OpenReader>);

impl std::fmt::Debug for BoundEvidenceReaderResolver {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("BoundEvidenceReaderResolver")
    }
}

impl BoundEvidenceReaderResolver {
    pub fn new(
        opener: impl Fn(&Path, &str) -> io::Result<Box<dyn EvidenceReader>> + Send + Sync + 'static,
    ) -> Self {
        Self(Arc::new(opener))
    }
}

pub(super) fn read_header(
    conn: &rusqlite::Connection,
    case_id: &str,
    source_id: &DataSourceId,
    cache: &RefCell<HashMap<String, Value>>,
    resolver: &BoundEvidenceReaderResolver,
    file_id: &FileEntryId,
    max_bytes: usize,
) -> Result<Vec<u8>, FileServiceError> {
    if case_id.is_empty() {
        return Err(FileServiceError::security(
            "Bound analysis reader requires a case identity",
        ));
    }
    let mut context = BoundHeaderContext {
        conn,
        case_id,
        source_id,
        cache,
        resolver,
    };
    viewer::read_file_header_with_context(&mut context, file_id, max_bytes)
}

struct BoundHeaderContext<'a> {
    conn: &'a rusqlite::Connection,
    case_id: &'a str,
    source_id: &'a DataSourceId,
    cache: &'a RefCell<HashMap<String, Value>>,
    resolver: &'a BoundEvidenceReaderResolver,
}

impl PreviewReadContext for BoundHeaderContext<'_> {
    fn conn(&self) -> &rusqlite::Connection {
        self.conn
    }
    fn case_id(&self) -> &str {
        self.case_id
    }
    fn requires_bound_reads(&self) -> bool {
        true
    }
    fn get_cached_preview_descriptor(&mut self, key: &str) -> Option<Value> {
        self.cache.borrow().get(key).cloned()
    }
    fn set_cached_preview_descriptor(&mut self, key: &str, value: &Value) {
        let mut cache = self.cache.borrow_mut();
        if cache.len() >= super::source_read::MAX_SOURCE_DESCRIPTOR_CACHE_ENTRIES
            && !cache.contains_key(key)
        {
            cache.clear();
        }
        cache.insert(key.into(), value.clone());
    }
    fn open_evidence_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
    ) -> Result<Box<dyn EvidenceReader>, FileServiceError> {
        self.validate_source(descriptor)?;
        (self.resolver.0)(Path::new(&descriptor.source_path), &descriptor.source_kind)
            .map_err(FileServiceError::Io)
    }
    fn open_candidate_block_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
        candidate: &PreviewPartitionCandidate,
    ) -> Result<(Box<dyn EvidenceReader>, u64, String), FileServiceError> {
        self.validate_source(descriptor)?;
        let source_path = Path::new(&descriptor.source_path);
        let reader = if let Some(identity) = &candidate.lvm_identity {
            super::viewer::LvmPoolRequestCache::new().open_volume_with_source_reader(
                source_path,
                identity,
                &mut |path, source| {
                    let kind = source.map_or(descriptor.source_kind.as_str(), |source| {
                        source.source_kind.as_str()
                    });
                    (self.resolver.0)(path, kind)
                },
            )?
        } else {
            (self.resolver.0)(source_path, &descriptor.source_kind)?
        };
        let offset = if candidate.lvm_identity.is_some() {
            0
        } else {
            candidate.offset
        };
        Ok((reader, offset, candidate.filesystem_kind.clone()))
    }
}

impl BoundHeaderContext<'_> {
    fn validate_source(&self, descriptor: &PreviewDescriptor) -> Result<(), FileServiceError> {
        if descriptor.data_source_id != self.source_id.0 {
            return Err(FileServiceError::security(
                "Analysis file belongs to another source",
            ));
        }
        Ok(())
    }
}
