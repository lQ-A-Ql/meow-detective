use crate::ResourceType;

/// Metadata returned by the native directory reader without loading payloads.
#[derive(Debug, Clone)]
pub struct IconDirMetadata {
    /// Resource type declared by the directory.
    pub resource_type: ResourceType,
    /// Image entries in source order.
    pub entries: Vec<IconDirEntryMetadata>,
}

/// A native directory entry and its declared source span.
#[derive(Debug, Clone)]
pub struct IconDirEntryMetadata {
    /// Declared width, with zero represented as 256.
    pub width: u32,
    /// Declared height, with zero represented as 256.
    pub height: u32,
    /// Source offset of the image payload.
    pub offset: u32,
    /// Declared size of the image payload.
    pub length: u32,
}
