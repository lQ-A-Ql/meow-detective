#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileExtractionProgressPhase {
    Copying,
    Finalizing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileExtractionProgressUpdate {
    pub phase: FileExtractionProgressPhase,
    pub bytes_written: u64,
    pub total_bytes: Option<u64>,
}

pub type FileExtractionProgressCallback<'a> = &'a mut dyn FnMut(FileExtractionProgressUpdate);
