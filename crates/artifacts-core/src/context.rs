use domain::FileEntryId;
use std::io;

pub struct ArtifactContext {
    pub file_id: FileEntryId,
    pub file_path: String,
    pub reader: Box<dyn io::Read>,
}

pub struct ArtifactCompanion {
    pub file_id: FileEntryId,
    pub file_path: String,
    pub data: Vec<u8>,
}
