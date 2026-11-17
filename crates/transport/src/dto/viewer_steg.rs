//! Bounded statistical steganalysis (S1) DTOs.

use serde::{Deserialize, Serialize};

use super::viewer::ImageIssueSeverityDto;

/// Triage result for one image file. Status-only failures leave the evidence
/// vectors empty and carry a stable `reason` code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStegDto {
    pub status: ImageStegStatusDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub global: Option<ImageStegAssessmentDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dominant_plane: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planes: Vec<ImageStegPlaneDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocks: Vec<ImageStegBlockDto>,
    pub blocks_scored: u32,
    pub blocks_total: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<ImageStegWarningDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageStegStatusDto {
    Completed,
    Unsupported,
    Oversized,
    Corrupt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageStegBandDto {
    Low,
    Medium,
    High,
    Inconclusive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ImageStegMethodKindDto {
    ChiSquare,
    Rs,
    Spa,
}

/// One detector applied to one plane or block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStegMethodDto {
    pub method: ImageStegMethodKindDto,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimate: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub samples: u64,
}

/// Fused assessment for a plane or for the whole image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStegAssessmentDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub score: Option<f64>,
    pub band: ImageStegBandDto,
    pub methods_used: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub methods: Vec<ImageStegMethodDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStegPlaneDto {
    pub plane: String,
    pub samples: u64,
    pub unique_values: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub assessment: Option<ImageStegAssessmentDto>,
}

/// Local block whose fused score reached the hotspot floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStegBlockDto {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub plane: String,
    pub score: f64,
    pub band: ImageStegBandDto,
    pub methods_used: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStegWarningDto {
    pub code: String,
    pub severity: ImageIssueSeverityDto,
    pub message: String,
}

#[cfg(test)]
#[path = "../../tests/unit/dto/viewer_steg.rs"]
mod tests;
