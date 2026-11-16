//! Mapping from analyzer reports to the transport DTO contract.

use transport::dto::viewer::{
    ImageExifBlockDto, ImageIssueSeverityDto, ImageMetadataDto, ImageMetadataStatusDto,
    ImageMetadataWarningDto, ImageSegmentDto, ImageStructureDto, ImageStructureFindingDto,
    ImageStructureHashesDto,
};

use super::{
    ExifBlockReport, ImageAnalysisReport, ImageAnalysisStatus, ImageIssueSeverity,
    ImageStructureReport, ImageWarning,
};

pub fn to_dto(report: ImageAnalysisReport) -> ImageMetadataDto {
    let ImageAnalysisReport {
        format,
        status,
        dimensions,
        exif,
        exif_blocks,
        warnings,
        structure,
        ..
    } = report;
    let (width, height) =
        dimensions.map_or((None, None), |(width, height)| (Some(width), Some(height)));
    ImageMetadataDto {
        status: status_dto(status),
        format: format.map(|format| format.as_str().to_string()),
        width,
        height,
        orientation: exif.orientation,
        make: exif.make,
        model: exif.model,
        software: exif.software,
        date_time_original: exif.date_time_original,
        create_date: exif.create_date,
        modify_date: exif.modify_date,
        lens_model: exif.lens_model,
        latitude: exif.latitude,
        longitude: exif.longitude,
        altitude: exif.altitude,
        gps_date_time: exif.gps_date_time,
        warnings: warnings.into_iter().map(warning_dto).collect(),
        exif_blocks: exif_blocks.into_iter().map(block_dto).collect(),
        structure: structure.map(structure_dto),
    }
}

fn status_dto(status: ImageAnalysisStatus) -> ImageMetadataStatusDto {
    match status {
        ImageAnalysisStatus::Present => ImageMetadataStatusDto::Present,
        ImageAnalysisStatus::Absent => ImageMetadataStatusDto::Absent,
        ImageAnalysisStatus::Unsupported => ImageMetadataStatusDto::Unsupported,
        ImageAnalysisStatus::Corrupt => ImageMetadataStatusDto::Corrupt,
        ImageAnalysisStatus::Oversized => ImageMetadataStatusDto::Oversized,
    }
}

fn severity_dto(severity: ImageIssueSeverity) -> ImageIssueSeverityDto {
    match severity {
        ImageIssueSeverity::Info => ImageIssueSeverityDto::Info,
        ImageIssueSeverity::Warning => ImageIssueSeverityDto::Warning,
        ImageIssueSeverity::Error => ImageIssueSeverityDto::Error,
    }
}

fn warning_dto(warning: ImageWarning) -> ImageMetadataWarningDto {
    ImageMetadataWarningDto {
        code: warning.code.to_string(),
        severity: severity_dto(warning.severity),
        message: warning.message.to_string(),
        offset: warning.offset,
    }
}

fn block_dto(block: ExifBlockReport) -> ImageExifBlockDto {
    ImageExifBlockDto {
        source: block.source.to_string(),
        offset: block.offset,
        length: block.length,
        decoded: block.decoded,
        field_count: block.field_count,
        warnings: block.warnings.into_iter().map(warning_dto).collect(),
    }
}

fn structure_dto(structure: ImageStructureReport) -> ImageStructureDto {
    ImageStructureDto {
        declared_end: structure.declared_end,
        trailing_bytes: structure.trailing_bytes,
        truncated: structure.truncated,
        segment_count: structure.segment_count,
        segments: structure
            .segments
            .into_iter()
            .map(|segment| ImageSegmentDto {
                kind: segment.kind,
                offset: segment.offset,
                length: segment.length,
                detail: segment.detail,
            })
            .collect(),
        segments_truncated: structure.segments_truncated,
        findings: structure
            .findings
            .into_iter()
            .map(|finding| ImageStructureFindingDto {
                code: finding.code.to_string(),
                severity: severity_dto(finding.severity),
                offset: finding.offset,
                detail: finding.detail,
            })
            .collect(),
        hashes: ImageStructureHashesDto {
            whole_file_sha256: structure.hashes.whole_file_sha256,
            trailing_sha256: structure.hashes.trailing_sha256,
            trailing_hashed_bytes: structure.hashes.trailing_hashed_bytes,
        },
    }
}
