//! Bounded entry point: format detection, scanner dispatch, and status derivation.

use std::sync::atomic::AtomicBool;

use crate::file_service::FileServiceError;

use super::postprocess::{apply_hashes, finish_structure};
use super::source::{ScanError, ScanReader};
use super::types::{
    ExifFields, ImageAnalysisReport, ImageAnalysisStatus, ImageFormat, ImageStructureReport,
};
use super::{bmp, gif, ico, jpeg, limits, png, tiff, webp};

#[derive(Debug, Clone, Copy)]
pub struct ImageAnalyzeOptions {
    pub random_read_limit: usize,
    pub scan_limit: usize,
    pub include_hashes: bool,
    pub dimensions_only: bool,
}

impl ImageAnalyzeOptions {
    #[must_use]
    pub const fn viewer() -> Self {
        Self {
            random_read_limit: limits::VIEWER_RANDOM_READ_LIMIT,
            scan_limit: limits::VIEWER_SCAN_LIMIT,
            include_hashes: false,
            dimensions_only: false,
        }
    }

    #[must_use]
    pub const fn analysis() -> Self {
        Self {
            random_read_limit: limits::ANALYSIS_RANDOM_READ_LIMIT,
            scan_limit: limits::ANALYSIS_SCAN_LIMIT,
            include_hashes: true,
            dimensions_only: false,
        }
    }

    #[must_use]
    pub const fn dimensions() -> Self {
        Self {
            random_read_limit: limits::VIEWER_RANDOM_READ_LIMIT,
            scan_limit: 0,
            include_hashes: false,
            dimensions_only: true,
        }
    }
}

pub trait ImageByteSource {
    fn size(&self) -> u64;
    fn read_at(&mut self, offset: u64, length: usize) -> Result<Vec<u8>, FileServiceError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ImageStructureError {
    #[error(transparent)]
    Read(#[from] FileServiceError),
    #[error("image structure analysis cancelled")]
    Cancelled,
}

impl ImageStructureError {
    #[must_use]
    pub fn into_file_service_error(self) -> FileServiceError {
        match self {
            Self::Read(error) => error,
            Self::Cancelled => FileServiceError::other("image structure analysis cancelled"),
        }
    }
}

pub fn analyze_image(
    source: &mut dyn ImageByteSource,
    options: &ImageAnalyzeOptions,
    cancel: &AtomicBool,
) -> Result<ImageAnalysisReport, ImageStructureError> {
    let mut reader = ScanReader::new(source, cancel, options);
    let size = reader.size();
    if size == 0 {
        return Ok(ImageAnalysisReport::empty(ImageAnalysisStatus::Corrupt));
    }
    let header = match reader.read(0, size.min(16) as usize) {
        Ok(bytes) => bytes,
        Err(ScanError::Read(error)) => return Err(error.into()),
        Err(ScanError::Cancelled) => return Err(ImageStructureError::Cancelled),
        Err(error) => {
            let mut report = ImageAnalysisReport::empty(ImageAnalysisStatus::Absent);
            apply_error(&mut report, error);
            report.bytes_read = reader.bytes_read;
            return Ok(report);
        }
    };
    let Some(format) = detect_format(&header) else {
        return Ok(ImageAnalysisReport::empty(ImageAnalysisStatus::Unsupported));
    };
    let mut report = ImageAnalysisReport::empty(ImageAnalysisStatus::Absent);
    report.format = Some(format);
    report.structure = Some(ImageStructureReport::default());
    let result = match format {
        ImageFormat::Jpeg => jpeg::scan(&mut reader, &mut report, options),
        ImageFormat::Png => png::scan(&mut reader, &mut report, options),
        ImageFormat::Webp => webp::scan(&mut reader, &mut report, options),
        ImageFormat::Gif => gif::scan(&mut reader, &mut report, options),
        ImageFormat::Bmp => bmp::scan(&mut reader, &mut report, options),
        ImageFormat::Ico => ico::scan(&mut reader, &mut report, options),
        ImageFormat::Tiff => tiff::scan(&mut reader, &mut report, options),
    };
    match result {
        Ok(()) => {
            if !options.dimensions_only {
                finish_structure(&mut report, size);
                report.status = derive_status(&report);
                if options.include_hashes {
                    apply_hashes(&mut reader, &mut report);
                }
            } else {
                report.warnings.clear();
                report.exif_blocks.clear();
                report.exif = ExifFields::default();
                report.structure = None;
                report.status = if report.dimensions.is_some() {
                    ImageAnalysisStatus::Absent
                } else {
                    ImageAnalysisStatus::Corrupt
                };
            }
        }
        Err(ScanError::Read(error)) => return Err(error.into()),
        Err(ScanError::Cancelled) => return Err(ImageStructureError::Cancelled),
        Err(error) => {
            apply_error(&mut report, error);
        }
    }
    report.bytes_read = reader.bytes_read;
    Ok(report)
}

fn apply_error(report: &mut ImageAnalysisReport, error: ScanError) {
    match error {
        ScanError::Corrupt => report.status = ImageAnalysisStatus::Corrupt,
        ScanError::Limit => report.status = ImageAnalysisStatus::Oversized,
        ScanError::Read(_) | ScanError::Cancelled => {}
    }
}

fn detect_format(header: &[u8]) -> Option<ImageFormat> {
    if header.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(ImageFormat::Jpeg);
    }
    if header.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(ImageFormat::Png);
    }
    if header.len() >= 12 && &header[..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        return Some(ImageFormat::Webp);
    }
    if header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a") {
        return Some(ImageFormat::Gif);
    }
    if header.starts_with(b"BM") {
        return Some(ImageFormat::Bmp);
    }
    if header.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some(ImageFormat::Ico);
    }
    if header.starts_with(b"II*\0") || header.starts_with(b"MM\0*") {
        return Some(ImageFormat::Tiff);
    }
    None
}

fn derive_status(report: &ImageAnalysisReport) -> ImageAnalysisStatus {
    if report.exif_blocks.iter().any(|block| block.decoded) {
        return ImageAnalysisStatus::Present;
    }
    if report.exif_blocks.is_empty() {
        return ImageAnalysisStatus::Absent;
    }
    let limit_only = report.exif_blocks.iter().all(|block| {
        block.warnings.iter().any(|warning| {
            matches!(
                warning.code,
                "image.exif.limit" | "image.exif.block-too-large"
            )
        })
    });
    if limit_only {
        ImageAnalysisStatus::Oversized
    } else {
        ImageAnalysisStatus::Corrupt
    }
}
