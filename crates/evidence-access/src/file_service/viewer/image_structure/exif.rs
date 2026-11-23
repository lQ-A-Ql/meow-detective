//! Bounded native EXIF decoding with per-field warnings.

use exif::{Exif, In, Tag, Value};

use super::limits::{EXIF_BLOCK_LIMIT, EXIF_BLOCK_READ_LIMIT, IFD_ENTRY_LIMIT, IFD_LIMIT};
use super::{
    push_warning, ExifBlockReport, ExifFields, ImageAnalysisReport, ImageIssueSeverity,
    ImageWarning,
};

pub(super) fn decode_block(
    report: &mut ImageAnalysisReport,
    source: &'static str,
    offset: u64,
    bytes: Vec<u8>,
) {
    let length = bytes.len() as u64;
    if report.exif_blocks.len() >= EXIF_BLOCK_LIMIT || bytes.len() > EXIF_BLOCK_READ_LIMIT {
        record_skipped_block(report, source, offset, length);
        return;
    }
    record_parsed_block(report, source, offset, length, read_tiff(bytes));
}

pub(super) fn read_tiff(bytes: Vec<u8>) -> Result<Exif, exif::Error> {
    exif::Reader::new()
        .limits(exif::ParseLimits {
            max_directories: IFD_LIMIT,
            max_entries: IFD_ENTRY_LIMIT,
            max_value_bytes: 4096,
            max_total_value_bytes: super::limits::VIEWER_RANDOM_READ_LIMIT,
        })
        .read_raw(bytes)
}

pub(super) fn record_parsed_block(
    report: &mut ImageAnalysisReport,
    source: &'static str,
    offset: u64,
    length: u64,
    parsed: Result<Exif, exif::Error>,
) {
    if report.exif_blocks.len() >= EXIF_BLOCK_LIMIT {
        push_warning(
            report,
            "image.exif.block-limit",
            ImageIssueSeverity::Warning,
            "additional EXIF blocks were ignored",
            Some(offset),
        );
        return;
    }
    let mut block_warnings = Vec::new();
    let mut decoded = false;
    let mut field_count = 0;
    match parsed {
        Err(exif::Error::TooBig(_)) => {
            block_warnings.push(block_warning("image.exif.limit", offset));
        }
        Ok(parsed) => {
            decoded = true;
            let mut field_warnings = Vec::new();
            let fields = interpret(&parsed, &mut field_warnings);
            field_count = fields.field_count();
            report.warnings.extend(field_warnings);
            if report.exif.is_empty() {
                report.exif = fields;
            }
        }
        Err(_) => block_warnings.push(block_warning("image.exif.decode-failed", offset)),
    }
    let successful_before = report
        .exif_blocks
        .iter()
        .filter(|block| block.decoded)
        .count();
    report.exif_blocks.push(ExifBlockReport {
        source,
        offset,
        length,
        decoded,
        field_count,
        warnings: block_warnings,
    });
    if decoded && successful_before == 1 {
        push_warning(
            report,
            "image.exif.multiple-blocks",
            ImageIssueSeverity::Info,
            "multiple EXIF blocks are present",
            None,
        );
    }
}

pub(super) fn record_skipped_block(
    report: &mut ImageAnalysisReport,
    source: &'static str,
    offset: u64,
    length: u64,
) {
    if report.exif_blocks.len() >= EXIF_BLOCK_LIMIT {
        push_warning(
            report,
            "image.exif.block-limit",
            ImageIssueSeverity::Warning,
            "additional EXIF blocks were ignored",
            Some(offset),
        );
        return;
    }
    push_warning(
        report,
        "image.exif.block-too-large",
        ImageIssueSeverity::Warning,
        "an EXIF block exceeded the decode limit",
        Some(offset),
    );
    report.exif_blocks.push(ExifBlockReport {
        source,
        offset,
        length,
        decoded: false,
        field_count: 0,
        warnings: vec![block_warning("image.exif.block-too-large", offset)],
    });
}

fn block_warning(code: &'static str, offset: u64) -> ImageWarning {
    ImageWarning {
        code,
        severity: ImageIssueSeverity::Warning,
        message: "an EXIF block could not be decoded",
        offset: Some(offset),
    }
}

fn field_warning(code: &'static str, message: &'static str) -> ImageWarning {
    ImageWarning {
        code,
        severity: ImageIssueSeverity::Warning,
        message,
        offset: None,
    }
}

fn interpret(exif: &Exif, warnings: &mut Vec<ImageWarning>) -> ExifFields {
    let mut fields = ExifFields::default();
    if let Some(value) = integer(exif, Tag::Orientation) {
        match u16::try_from(value) {
            Ok(value @ 1..=8) => fields.orientation = Some(value),
            _ => warnings.push(field_warning(
                "image.exif.field-invalid",
                "the EXIF orientation value is invalid",
            )),
        }
    }
    fields.make = text(exif, Tag::Make, warnings);
    fields.model = text(exif, Tag::Model, warnings);
    fields.software = text(exif, Tag::Software, warnings);
    fields.lens_model = text(exif, Tag::LensModel, warnings);
    fields.date_time_original = text(exif, Tag::DateTimeOriginal, warnings);
    fields.create_date = text(exif, Tag::DateTimeDigitized, warnings);
    fields.modify_date = text(exif, Tag::DateTime, warnings);
    fields.latitude = coordinate(
        exif,
        Tag::GPSLatitude,
        Tag::GPSLatitudeRef,
        "N",
        "S",
        90.0,
        warnings,
    );
    fields.longitude = coordinate(
        exif,
        Tag::GPSLongitude,
        Tag::GPSLongitudeRef,
        "E",
        "W",
        180.0,
        warnings,
    );
    fields.altitude = altitude(exif, warnings);
    fields.gps_date_time = gps_date_time(exif, warnings);
    fields
}

fn integer(exif: &Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY)?.value.get_uint(0)
}

fn text(exif: &Exif, tag: Tag, warnings: &mut Vec<ImageWarning>) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let Value::Ascii(values) = &field.value else {
        warnings.push(field_warning(
            "image.exif.field-invalid",
            "an EXIF text field has an unexpected type",
        ));
        return None;
    };
    let first = values.first()?;
    match std::str::from_utf8(first) {
        Ok(value) => {
            let value = value.trim_end_matches('\0').trim();
            (!value.is_empty()).then(|| value.to_owned())
        }
        Err(_) => {
            warnings.push(field_warning(
                "image.exif.field-invalid",
                "an EXIF text field is not valid UTF-8",
            ));
            None
        }
    }
}

fn text_value(exif: &Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let Value::Ascii(values) = &field.value else {
        return None;
    };
    let value = std::str::from_utf8(values.first()?).ok()?;
    let value = value.trim_end_matches('\0').trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn rationals(exif: &Exif, tag: Tag) -> Option<Result<Vec<f64>, ()>> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let Value::Rational(values) = &field.value else {
        return Some(Err(()));
    };
    values
        .iter()
        .map(|value| {
            if value.denom == 0 {
                Err(())
            } else {
                Ok(f64::from(value.num) / f64::from(value.denom))
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .into()
}

fn rational_values(exif: &Exif, tag: Tag, warnings: &mut Vec<ImageWarning>) -> Option<Vec<f64>> {
    match rationals(exif, tag)? {
        Ok(values) => Some(values),
        Err(()) => {
            warnings.push(field_warning(
                "image.exif.field-invalid",
                "an EXIF rational field is invalid",
            ));
            None
        }
    }
}

fn coordinate(
    exif: &Exif,
    tag: Tag,
    reference: Tag,
    positive: &str,
    negative: &str,
    limit: f64,
    warnings: &mut Vec<ImageWarning>,
) -> Option<f64> {
    let values = rational_values(exif, tag, warnings)?;
    if values.len() != 3 || values[1] >= 60.0 || values[2] >= 60.0 {
        warnings.push(field_warning(
            "image.exif.gps-invalid",
            "an EXIF GPS coordinate is out of range",
        ));
        return None;
    }
    let coordinate = values[0] + values[1] / 60.0 + values[2] / 3600.0;
    if coordinate > limit {
        warnings.push(field_warning(
            "image.exif.gps-invalid",
            "an EXIF GPS coordinate is out of range",
        ));
        return None;
    }
    match text_value(exif, reference).as_deref() {
        Some(value) if value == positive => Some(coordinate),
        Some(value) if value == negative => Some(-coordinate),
        _ => {
            warnings.push(field_warning(
                "image.exif.gps-invalid",
                "an EXIF GPS reference is missing or invalid",
            ));
            None
        }
    }
}

fn altitude(exif: &Exif, warnings: &mut Vec<ImageWarning>) -> Option<f64> {
    let values = rational_values(exif, Tag::GPSAltitude, warnings)?;
    if values.len() != 1 {
        warnings.push(field_warning(
            "image.exif.gps-invalid",
            "an EXIF GPS altitude is invalid",
        ));
        return None;
    }
    match integer(exif, Tag::GPSAltitudeRef) {
        Some(0) => Some(values[0]),
        Some(1) => Some(-values[0]),
        _ => {
            warnings.push(field_warning(
                "image.exif.gps-invalid",
                "an EXIF GPS altitude reference is missing or invalid",
            ));
            None
        }
    }
}

fn gps_date_time(exif: &Exif, warnings: &mut Vec<ImageWarning>) -> Option<String> {
    let values = rational_values(exif, Tag::GPSTimeStamp, warnings)?;
    let Some(date) = text_value(exif, Tag::GPSDateStamp) else {
        warnings.push(field_warning(
            "image.exif.gps-invalid",
            "an EXIF GPS date is missing",
        ));
        return None;
    };
    if values.len() != 3
        || values[0] >= 24.0
        || values[1] >= 60.0
        || values[2] >= 60.0
        || values[0].fract() != 0.0
        || values[1].fract() != 0.0
    {
        warnings.push(field_warning(
            "image.exif.gps-invalid",
            "an EXIF GPS timestamp is out of range",
        ));
        return None;
    }
    let date = chrono::NaiveDate::parse_from_str(&date, "%Y:%m:%d").ok()?;
    let nanos = (values[2].fract() * 1_000_000_000.0).round() as u32;
    let time =
        date.and_hms_nano_opt(values[0] as u32, values[1] as u32, values[2] as u32, nanos)?;
    Some(time.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string())
}
