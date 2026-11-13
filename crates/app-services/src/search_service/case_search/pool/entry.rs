use crate::resource_budget::ResourcePermit;
use search::{FileSearchOptions, FileSearchQuerySession, SearchIndex};
use std::{ops::Deref, path::PathBuf, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct IndexKey {
    pub case_root: PathBuf,
    pub case_id: String,
    pub source_id: String,
    pub index_dir: PathBuf,
    pub generation: String,
    pub schema: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct QueryKey {
    pub index: IndexKey,
    pub opstamp: u64,
    pub options: FileSearchOptions,
}

pub(super) struct CachedIndex {
    pub index: SearchIndex,
    pub _permit: ResourcePermit,
}

pub(crate) struct CachedSession {
    pub(super) session: FileSearchQuerySession,
    pub(super) _index: Arc<CachedIndex>,
    pub(super) _permit: ResourcePermit,
}

impl Deref for CachedSession {
    type Target = FileSearchQuerySession;

    fn deref(&self) -> &Self::Target {
        &self.session
    }
}
