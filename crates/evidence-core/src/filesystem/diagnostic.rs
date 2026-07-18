#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileSystemDiagnosticKind {
    DirectoryPartial,
    DirectoryUnreadable,
    EntryUnavailable,
    MetadataDegraded,
    TypeConflict,
}

impl FileSystemDiagnosticKind {
    pub const fn affects_catalog_completeness(self) -> bool {
        matches!(
            self,
            Self::DirectoryPartial | Self::DirectoryUnreadable | Self::EntryUnavailable
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSystemDiagnostic {
    pub kind: FileSystemDiagnosticKind,
    pub path: Option<String>,
    pub inode: Option<u64>,
    pub message: String,
}

impl FileSystemDiagnostic {
    pub fn new(kind: FileSystemDiagnosticKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            path: None,
            inode: None,
            message: message.into(),
        }
    }

    pub fn with_inode(mut self, inode: u64) -> Self {
        self.inode = Some(inode);
        self
    }

    pub fn with_default_path(mut self, path: &str) -> Self {
        if self.path.is_none() {
            self.path = Some(path.to_string());
        }
        self
    }
}
