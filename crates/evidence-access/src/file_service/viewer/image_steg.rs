//! Source-bound statistical steganalysis orchestration for a single image.
//!
//! The container format comes from the shared magic classifier, never from the
//! file name. The analyser receives the complete (bounded) file bytes and never
//! writes evidence.

use image_inspection::{
    analyze, StegAssessment, StegBand, StegFormat, StegLimits, StegMethod, StegMethodReport,
    StegPlaneReport, StegReport, StegSeverity, StegStatus, StegWarning,
};
use transport::dto::viewer_steg::{
    ImageStegAssessmentDto, ImageStegBandDto, ImageStegBlockDto, ImageStegDto, ImageStegMethodDto,
    ImageStegMethodKindDto, ImageStegPlaneDto, ImageStegStatusDto, ImageStegWarningDto,
};
use transport::dto::ImageIssueSeverityDto;

use super::preview_bytes::read_inline_preview_bytes_for_file;
use super::{detect_preview_for_file, open_file_handle_real, PreviewReadContext};
use crate::file_service::FileServiceError;

pub fn image_steg_for_file<C>(
    mut context: C,
    file_id: &str,
) -> Result<ImageStegDto, FileServiceError>
where
    C: PreviewReadContext,
{
    let handle = open_file_handle_real(&mut context, file_id)?;
    let detected = detect_preview_for_file(&mut context, file_id)?;
    let Some(format) = detected
        .as_ref()
        .and_then(|value| StegFormat::from_magic_mime(value.mime))
    else {
        return Ok(failed_dto(StegStatus::Unsupported, None, "image.format"));
    };
    let limits = StegLimits::default();
    if handle.size > limits.max_input_bytes {
        return Ok(failed_dto(
            StegStatus::Oversized,
            Some(format),
            "limit.bytes",
        ));
    }
    let bytes = read_inline_preview_bytes_for_file(&mut context, file_id, handle.size)?;
    Ok(to_dto(analyze(&bytes, format, &limits)))
}

fn failed_dto(
    status: StegStatus,
    format: Option<StegFormat>,
    reason: &'static str,
) -> ImageStegDto {
    ImageStegDto {
        status: status_dto(status),
        format: format.map(|value| value.as_str().to_string()),
        width: None,
        height: None,
        reason: Some(reason.to_string()),
        global: None,
        dominant_plane: None,
        planes: Vec::new(),
        blocks: Vec::new(),
        blocks_scored: 0,
        blocks_total: 0,
        warnings: Vec::new(),
    }
}

fn to_dto(report: StegReport) -> ImageStegDto {
    ImageStegDto {
        status: status_dto(report.status),
        format: report.format.map(|value| value.as_str().to_string()),
        width: report.width,
        height: report.height,
        reason: report.reason.map(str::to_string),
        global: report.global.as_ref().map(assessment_dto),
        dominant_plane: report
            .dominant_plane
            .map(|value| value.as_str().to_string()),
        planes: report.planes.iter().map(plane_dto).collect(),
        blocks: report.blocks.iter().map(block_dto).collect(),
        blocks_scored: report.blocks_scored,
        blocks_total: report.blocks_total,
        warnings: report.warnings.iter().map(warning_dto).collect(),
    }
}

fn status_dto(status: StegStatus) -> ImageStegStatusDto {
    match status {
        StegStatus::Completed => ImageStegStatusDto::Completed,
        StegStatus::Unsupported => ImageStegStatusDto::Unsupported,
        StegStatus::Oversized => ImageStegStatusDto::Oversized,
        StegStatus::Corrupt => ImageStegStatusDto::Corrupt,
    }
}

fn band_dto(band: StegBand) -> ImageStegBandDto {
    match band {
        StegBand::Low => ImageStegBandDto::Low,
        StegBand::Medium => ImageStegBandDto::Medium,
        StegBand::High => ImageStegBandDto::High,
        StegBand::Inconclusive => ImageStegBandDto::Inconclusive,
    }
}

fn method_kind_dto(method: StegMethod) -> ImageStegMethodKindDto {
    match method {
        StegMethod::ChiSquare => ImageStegMethodKindDto::ChiSquare,
        StegMethod::Rs => ImageStegMethodKindDto::Rs,
        StegMethod::Spa => ImageStegMethodKindDto::Spa,
    }
}

fn assessment_dto(assessment: &StegAssessment) -> ImageStegAssessmentDto {
    ImageStegAssessmentDto {
        score: assessment.score,
        band: band_dto(assessment.band),
        methods_used: assessment.methods_used,
        methods: assessment.methods.iter().map(method_dto).collect(),
    }
}

fn method_dto(method: &StegMethodReport) -> ImageStegMethodDto {
    ImageStegMethodDto {
        method: method_kind_dto(method.method),
        estimate: method.estimate,
        score: method.score,
        samples: method.samples,
    }
}

fn plane_dto(plane: &StegPlaneReport) -> ImageStegPlaneDto {
    ImageStegPlaneDto {
        plane: plane.plane.as_str().to_string(),
        samples: plane.samples,
        unique_values: plane.unique_values,
        assessment: plane.assessment.as_ref().map(assessment_dto),
    }
}

fn block_dto(block: &image_inspection::StegBlockReport) -> ImageStegBlockDto {
    ImageStegBlockDto {
        x: block.x,
        y: block.y,
        width: block.width,
        height: block.height,
        plane: block.plane.as_str().to_string(),
        score: block.score,
        band: band_dto(block.band),
        methods_used: block.methods_used,
    }
}

fn warning_dto(warning: &StegWarning) -> ImageStegWarningDto {
    ImageStegWarningDto {
        code: warning.code.to_string(),
        severity: match warning.severity {
            StegSeverity::Info => ImageIssueSeverityDto::Info,
            StegSeverity::Warning => ImageIssueSeverityDto::Warning,
        },
        message: warning.message.clone(),
    }
}
