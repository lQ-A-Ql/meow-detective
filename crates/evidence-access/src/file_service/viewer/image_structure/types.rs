//! Report and finding types shared by the image analyzer and its scanners.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Jpeg,
    Png,
    Webp,
    Gif,
    Bmp,
    Ico,
    Tiff,
}

impl ImageFormat {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Png => "png",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Bmp => "bmp",
            Self::Ico => "ico",
            Self::Tiff => "tiff",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageAnalysisStatus {
    Present,
    Absent,
    Unsupported,
    Corrupt,
    Oversized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageIssueSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct ImageWarning {
    pub code: &'static str,
    pub severity: ImageIssueSeverity,
    pub message: &'static str,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ImageFinding {
    pub code: &'static str,
    pub severity: ImageIssueSeverity,
    pub offset: Option<u64>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ExifFields {
    pub orientation: Option<u16>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub software: Option<String>,
    pub date_time_original: Option<String>,
    pub create_date: Option<String>,
    pub modify_date: Option<String>,
    pub lens_model: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub altitude: Option<f64>,
    pub gps_date_time: Option<String>,
}

impl ExifFields {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.field_count() == 0
    }

    #[must_use]
    pub fn field_count(&self) -> u32 {
        [
            self.orientation.is_some(),
            self.make.is_some(),
            self.model.is_some(),
            self.software.is_some(),
            self.date_time_original.is_some(),
            self.create_date.is_some(),
            self.modify_date.is_some(),
            self.lens_model.is_some(),
            self.latitude.is_some(),
            self.longitude.is_some(),
            self.altitude.is_some(),
            self.gps_date_time.is_some(),
        ]
        .into_iter()
        .filter(|present| *present)
        .count() as u32
    }
}

#[derive(Debug, Clone)]
pub struct ExifBlockReport {
    pub source: &'static str,
    pub offset: u64,
    pub length: u64,
    pub decoded: bool,
    pub field_count: u32,
    pub warnings: Vec<ImageWarning>,
}

#[derive(Debug, Clone, Default)]
pub struct ImageStructureHashes {
    pub whole_file_sha256: Option<String>,
    pub trailing_sha256: Option<String>,
    pub trailing_hashed_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ImageSegmentReport {
    pub kind: String,
    pub offset: u64,
    pub length: u64,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct ImageStructureReport {
    pub declared_end: Option<u64>,
    pub trailing_bytes: Option<u64>,
    pub truncated: bool,
    pub segments: Vec<ImageSegmentReport>,
    pub segment_count: u32,
    pub segments_truncated: bool,
    pub findings: Vec<ImageFinding>,
    pub hashes: ImageStructureHashes,
}

#[derive(Debug, Clone)]
pub struct ImageAnalysisReport {
    pub format: Option<ImageFormat>,
    pub status: ImageAnalysisStatus,
    pub dimensions: Option<(u32, u32)>,
    pub exif: ExifFields,
    pub exif_blocks: Vec<ExifBlockReport>,
    pub warnings: Vec<ImageWarning>,
    pub structure: Option<ImageStructureReport>,
    pub bytes_read: u64,
}

impl ImageAnalysisReport {
    #[must_use]
    pub fn empty(status: ImageAnalysisStatus) -> Self {
        Self {
            format: None,
            status,
            dimensions: None,
            exif: ExifFields::default(),
            exif_blocks: Vec::new(),
            warnings: Vec::new(),
            structure: None,
            bytes_read: 0,
        }
    }

    /// True when the report carries fields worth persisting as an artifact.
    #[must_use]
    pub fn has_forensic_content(&self) -> bool {
        if self
            .exif_blocks
            .iter()
            .any(|block| block.decoded && block.field_count > 0)
        {
            return true;
        }
        if !self.warnings.is_empty() {
            return true;
        }
        let Some(structure) = self.structure.as_ref() else {
            return false;
        };
        structure.truncated
            || structure.findings.iter().any(|finding| {
                matches!(
                    finding.severity,
                    ImageIssueSeverity::Warning | ImageIssueSeverity::Error
                )
            })
    }
}
