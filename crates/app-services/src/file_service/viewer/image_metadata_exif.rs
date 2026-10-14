//! Strict EXIF field interpretation; the library follows Exif/GPS child IFDs.

use super::image_metadata_input::MetadataInputError;
use exif::{Exif, In, Tag, Value};
use transport::dto::ImageMetadataDto;

pub(super) fn decode(
    bytes: Vec<u8>,
    output: &mut ImageMetadataDto,
) -> Result<(), MetadataInputError> {
    super::image_metadata_budget::validate_budget(&bytes)?;
    let exif = exif::Reader::new()
        .read_raw(bytes)
        .map_err(|_| MetadataInputError::Corrupt)?;
    output.orientation =
        integer(&exif, Tag::Orientation).and_then(|value| u16::try_from(value).ok());
    output.width = output
        .width
        .or_else(|| integer(&exif, Tag::PixelXDimension))
        .or_else(|| integer(&exif, Tag::ImageWidth));
    output.height = output
        .height
        .or_else(|| integer(&exif, Tag::PixelYDimension))
        .or_else(|| integer(&exif, Tag::ImageLength));
    output.make = text(&exif, Tag::Make);
    output.model = text(&exif, Tag::Model);
    output.software = text(&exif, Tag::Software);
    output.lens_model = text(&exif, Tag::LensModel);
    output.date_time_original = text(&exif, Tag::DateTimeOriginal);
    output.create_date = text(&exif, Tag::DateTimeDigitized);
    output.modify_date = text(&exif, Tag::DateTime);
    output.latitude = coordinate(&exif, Tag::GPSLatitude, Tag::GPSLatitudeRef, "N", "S", 90.0)?;
    output.longitude = coordinate(
        &exif,
        Tag::GPSLongitude,
        Tag::GPSLongitudeRef,
        "E",
        "W",
        180.0,
    )?;
    if let Some(altitude) = rationals(&exif, Tag::GPSAltitude)? {
        if altitude.len() != 1 {
            return Err(MetadataInputError::Corrupt);
        }
        output.altitude = match integer(&exif, Tag::GPSAltitudeRef) {
            Some(0) => Some(altitude[0]),
            Some(1) => Some(-altitude[0]),
            _ => return Err(MetadataInputError::Corrupt),
        };
    }
    output.gps_date_time = gps_date_time(&exif)?;
    Ok(())
}

fn integer(exif: &Exif, tag: Tag) -> Option<u32> {
    exif.get_field(tag, In::PRIMARY)?.value.get_uint(0)
}

fn text(exif: &Exif, tag: Tag) -> Option<String> {
    let Value::Ascii(values) = &exif.get_field(tag, In::PRIMARY)?.value else {
        return None;
    };
    let value = std::str::from_utf8(values.first()?)
        .ok()?
        .trim_end_matches('\0')
        .trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn rationals(exif: &Exif, tag: Tag) -> Result<Option<Vec<f64>>, MetadataInputError> {
    let Some(field) = exif.get_field(tag, In::PRIMARY) else {
        return Ok(None);
    };
    let Value::Rational(values) = &field.value else {
        return Err(MetadataInputError::Corrupt);
    };
    values
        .iter()
        .map(|value| {
            if value.denom == 0 {
                Err(MetadataInputError::Corrupt)
            } else {
                Ok(f64::from(value.num) / f64::from(value.denom))
            }
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn coordinate(
    exif: &Exif,
    tag: Tag,
    reference: Tag,
    positive: &str,
    negative: &str,
    limit: f64,
) -> Result<Option<f64>, MetadataInputError> {
    let Some(values) = rationals(exif, tag)? else {
        return Ok(None);
    };
    if values.len() != 3 || values[1] >= 60.0 || values[2] >= 60.0 {
        return Err(MetadataInputError::Corrupt);
    }
    let coordinate = values[0] + values[1] / 60.0 + values[2] / 3600.0;
    if coordinate > limit {
        return Err(MetadataInputError::Corrupt);
    }
    match text(exif, reference).as_deref() {
        Some(value) if value == positive => Ok(Some(coordinate)),
        Some(value) if value == negative => Ok(Some(-coordinate)),
        _ => Err(MetadataInputError::Corrupt),
    }
}

fn gps_date_time(exif: &Exif) -> Result<Option<String>, MetadataInputError> {
    let Some(values) = rationals(exif, Tag::GPSTimeStamp)? else {
        return Ok(None);
    };
    let Some(date) = text(exif, Tag::GPSDateStamp) else {
        return Ok(None);
    };
    if values.len() != 3
        || values[0] >= 24.0
        || values[1] >= 60.0
        || values[2] >= 60.0
        || values[0].fract() != 0.0
        || values[1].fract() != 0.0
    {
        return Err(MetadataInputError::Corrupt);
    }
    let date = chrono::NaiveDate::parse_from_str(&date, "%Y:%m:%d")
        .map_err(|_| MetadataInputError::Corrupt)?;
    let nanos = (values[2].fract() * 1_000_000_000.0).round() as u32;
    let time = date
        .and_hms_nano_opt(values[0] as u32, values[1] as u32, values[2] as u32, nanos)
        .ok_or(MetadataInputError::Corrupt)?;
    Ok(Some(time.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string()))
}
