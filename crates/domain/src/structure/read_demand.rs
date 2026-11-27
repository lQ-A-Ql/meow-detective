use super::ByteRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadIntent {
    MetadataInspection,
    ContentRead,
}

/// This capability requires content verification during every underlying read.
/// It does not assert that all requested bytes have already been consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RangeReadCapability {
    pub range: ByteRange,
    pub target_size: u64,
}

/// The identity describes an existing content-bound native plan. Registration
/// into a case source revision remains a separate lifecycle operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadDemand {
    pub target_fingerprint: String,
    pub intent: ReadIntent,
    pub capability: RangeReadCapability,
}
