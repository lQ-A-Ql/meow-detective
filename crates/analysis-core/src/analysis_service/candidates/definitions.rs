//! Evidence category definitions and their candidate discovery strategy.

use super::common::EvidencePathPattern;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvidenceDiscovery {
    PathPatterns,
    Magic(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy)]
pub struct EvidenceCategoryDef {
    pub category: &'static str,
    pub display_name: &'static str,
    pub evidence_kind: &'static str,
    pub parser: &'static str,
    pub artifact_families: &'static [&'static str],
    pub(super) patterns: &'static [EvidencePathPattern],
    pub(super) matcher: Option<fn(&str) -> bool>,
    pub discovery: EvidenceDiscovery,
}

pub(super) const EMAIL_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "Email",
    display_name: "电子邮件",
    evidence_kind: "email",
    parser: "email",
    artifact_families: &["EmailMessage", "EmailAccount"],
    patterns: &[
        EvidencePathPattern::Suffix(".eml"),
        EvidencePathPattern::Suffix(".emlx"),
        EvidencePathPattern::Suffix(".mbox"),
        EvidencePathPattern::FoxmailBox,
        EvidencePathPattern::FoxmailIndex,
        EvidencePathPattern::FoxmailAccount,
        EvidencePathPattern::Suffix(".pst"),
        EvidencePathPattern::Suffix(".ost"),
    ],
    matcher: None,
    discovery: EvidenceDiscovery::PathPatterns,
};

pub(super) const FILE_TYPE_INVENTORY_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "FileTypeInventory",
    display_name: "File type inventory",
    evidence_kind: "metadata_inventory",
    parser: "metadata.extension_path",
    artifact_families: &[],
    patterns: &[],
    matcher: None,
    discovery: EvidenceDiscovery::PathPatterns,
};

pub(crate) const IMAGE_METADATA_CATEGORY_DEF: EvidenceCategoryDef = EvidenceCategoryDef {
    category: "ImageMetadata",
    display_name: "Image metadata",
    evidence_kind: "image_metadata",
    parser: "image.metadata.container",
    artifact_families: &["ImageMetadata"],
    patterns: &[],
    matcher: None,
    discovery: EvidenceDiscovery::Magic(&["JPEG", "PNG", "WEBP", "GIF", "BMP", "ICO", "TIFF"]),
};

pub(crate) const IMAGE_METADATA_PROBE_CATEGORY: &str = "ImageMetadataProbe";
