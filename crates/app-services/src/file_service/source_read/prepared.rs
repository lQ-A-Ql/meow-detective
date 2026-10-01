//! prepared support for source-bound reads.
use super::*;
impl PreparedSourceReadState {
    pub(crate) fn new(
        case_id: impl Into<String>,
        data_source_id: DataSourceId,
        derived_runtime: Arc<DerivedRbdRuntime>,
    ) -> Self {
        Self {
            case_id: case_id.into(),
            data_source_id,
            descriptors: HashMap::new(),
            derived_runtime,
            derived_reads: DerivedSourceReadCache::default(),
        }
    }

    pub(crate) fn read_file_header_by_id(
        &mut self,
        source_conn: &rusqlite::Connection,
        file_id: &FileEntryId,
        max_bytes: usize,
    ) -> Result<Vec<u8>, FileServiceError> {
        let descriptor = {
            let mut context = PreparedSourceReadContext {
                source_conn,
                state: self,
            };
            descriptor_for_file_with_cache(&mut context, file_id)?
        };
        self.derived_reads.read_file_header(
            source_conn,
            &self.data_source_id,
            &self.derived_runtime,
            &descriptor,
            max_bytes,
        )
    }
}

struct PreparedSourceReadContext<'a> {
    source_conn: &'a rusqlite::Connection,
    state: &'a mut PreparedSourceReadState,
}

impl PreviewReadContext for PreparedSourceReadContext<'_> {
    fn conn(&self) -> &rusqlite::Connection {
        self.source_conn
    }

    fn case_id(&self) -> &str {
        &self.state.case_id
    }

    fn get_cached_preview_descriptor(&mut self, key: &str) -> Option<Value> {
        self.state.descriptors.get(key).cloned()
    }

    fn set_cached_preview_descriptor(&mut self, key: &str, value: &Value) {
        cache_preview_descriptor(&mut self.state.descriptors, key, value);
    }

    fn open_evidence_reader(
        &mut self,
        descriptor: &PreviewDescriptor,
    ) -> Result<Box<dyn evidence_core::EvidenceReader>, FileServiceError> {
        if descriptor.data_source_id != self.state.data_source_id.0 {
            return Err(FileServiceError::security(
                "Evidence descriptor does not belong to the bound data source",
            ));
        }
        if descriptor.source_kind != "ceph_rbd" {
            return Err(FileServiceError::security(
                "Prepared derived-source reader received a non-RBD descriptor",
            ));
        }
        self.state
            .derived_runtime
            .open_reader()
            .map(|reader| Box::new(reader) as Box<dyn evidence_core::EvidenceReader>)
            .map_err(|error| FileServiceError::other(error.to_string()))
    }
}
