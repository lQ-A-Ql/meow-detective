use std::fmt;

/// Container format accepted by the analyser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StegFormat {
    Png,
    Bmp,
}

impl StegFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Bmp => "bmp",
        }
    }

    /// Maps a magic-detected MIME type to a supported container.
    pub fn from_magic_mime(mime: &str) -> Option<Self> {
        match mime.trim().to_ascii_lowercase().as_str() {
            "image/png" => Some(Self::Png),
            "image/bmp" => Some(Self::Bmp),
            _ => None,
        }
    }
}

impl fmt::Display for StegFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Outcome of one analysis request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StegStatus {
    Completed,
    Unsupported,
    Oversized,
    Corrupt,
}

/// One statistical detector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StegMethod {
    ChiSquare,
    Rs,
    Spa,
}

impl StegMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ChiSquare => "chiSquare",
            Self::Rs => "rs",
            Self::Spa => "spa",
        }
    }
}

/// Triage band derived from the fused 0..100 score.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StegBand {
    Low,
    Medium,
    High,
    Inconclusive,
}

impl StegBand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Inconclusive => "inconclusive",
        }
    }
}

/// Sample plane that was actually analysed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneKind {
    Gray,
    Red,
    Green,
    Blue,
    Index,
}

impl PlaneKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Gray => "gray",
            Self::Red => "r",
            Self::Green => "g",
            Self::Blue => "b",
            Self::Index => "index",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StegSeverity {
    Info,
    Warning,
}

/// Bounded, stable-code warning emitted during analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StegWarning {
    pub code: &'static str,
    pub severity: StegSeverity,
    pub message: String,
}

/// Evidence produced by one detector on one plane or block.
#[derive(Debug, Clone)]
pub struct StegMethodReport {
    pub method: StegMethod,
    /// Raw estimate: chi-square p-value, RS rate, or SPA rate.
    pub estimate: Option<f64>,
    /// Normalized suspicion score in 0..100.
    pub score: Option<f64>,
    pub samples: u64,
}

/// Fused assessment for a plane or for the whole image.
#[derive(Debug, Clone)]
pub struct StegAssessment {
    pub score: Option<f64>,
    pub band: StegBand,
    pub methods_used: u8,
    pub methods: Vec<StegMethodReport>,
}

#[derive(Debug, Clone)]
pub struct StegPlaneReport {
    pub plane: PlaneKind,
    pub samples: u64,
    pub unique_values: u32,
    pub assessment: Option<StegAssessment>,
}

/// Local hotspot for blocks whose fused score reached at least 10.
#[derive(Debug, Clone)]
pub struct StegBlockReport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub plane: PlaneKind,
    pub score: f64,
    pub band: StegBand,
    pub methods_used: u8,
}

/// Complete analysis result. Status-only failures leave the evidence vectors empty.
#[derive(Debug, Clone)]
pub struct StegReport {
    pub status: StegStatus,
    pub format: Option<StegFormat>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub planes: Vec<StegPlaneReport>,
    pub global: Option<StegAssessment>,
    pub dominant_plane: Option<PlaneKind>,
    pub blocks: Vec<StegBlockReport>,
    pub blocks_scored: u32,
    pub blocks_total: u32,
    pub warnings: Vec<StegWarning>,
    pub reason: Option<&'static str>,
}

impl StegReport {
    pub(crate) fn failed(
        status: StegStatus,
        format: Option<StegFormat>,
        dims: Option<(u32, u32)>,
        reason: &'static str,
    ) -> Self {
        Self {
            status,
            format,
            width: dims.map(|(w, _)| w),
            height: dims.map(|(_, h)| h),
            planes: Vec::new(),
            global: None,
            dominant_plane: None,
            blocks: Vec::new(),
            blocks_scored: 0,
            blocks_total: 0,
            warnings: Vec::new(),
            reason: Some(reason),
        }
    }
}
